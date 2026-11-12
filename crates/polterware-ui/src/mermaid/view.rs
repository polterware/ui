//! The view that shows a diagram: a scene, a viewport, and the pointer.
//!
//! It behaves like gpui-kit's `Canvas`/`Plot` views for navigation: drag empty space to pan,
//! scroll to pan, the secondary modifier (command on macOS, control elsewhere) with a scroll to
//! zoom around the pointer, a double-click to fit the view box, and the hovered or selected
//! element highlighted. GPUI 0.2.2 has no magnify gesture event, so a trackpad pinch arrives as
//! its scroll-with-secondary-modifier form; that is the zoom gesture here. Selecting a node or
//! edge emits [`MermaidEvent::Selected`] with the [`SceneHit`], so the app can turn a click back
//! into an edit of the diagram source.

use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    Bounds, Context, CursorStyle, DispatchPhase, Entity, EventEmitter, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Render, RenderImage, ScrollDelta,
    ScrollWheelEvent, Window, canvas, div, point, prelude::*,
};

use polterware_mermaid::scene::Scene;

use crate::mermaid::hit::{SceneHit, hit_test};
use crate::mermaid::paint::{ImageResolver, paint_scene, paint_selection};
use crate::mermaid::viewport::SceneViewport;
use crate::theme::Theme;

/// How many screen pixels around a thin edge still pick it.
const HIT_TOLERANCE: f64 = 4.0;
/// The padding an automatic fit leaves around the scene's view box.
const DEFAULT_FIT_PADDING: f32 = 24.0;
/// One wheel line, in pixels, when a platform reports scroll in lines.
const LINE_HEIGHT: f32 = 24.0;

/// What a [`MermaidView`] tells its app.
#[derive(Clone, Debug, PartialEq)]
pub enum MermaidEvent {
    /// The element under a click, or `None` when the click landed on empty space.
    Selected(Option<SceneHit>),
    /// The element under the pointer, when it changes.
    Hovered(Option<SceneHit>),
}

#[derive(Clone, Copy, Debug)]
struct Press {
    origin: Point<Pixels>,
    viewport: SceneViewport,
    panning: bool,
}

/// A diagram in a GPUI view: pan, zoom, fit, and hit testing over a
/// [`Scene`](polterware_mermaid::scene::Scene).
///
/// The look is the Mermaid SVG (the upstream renderers' shapes and labels), and the behavior is
/// gpui-kit's canvas views: drag to pan, scroll to pan, secondary-modifier scroll to zoom around
/// the pointer, and a double-click to fit. The scene is painted by GPUI paths, quads, text, and
/// images; GPUI 0.2.2 clips content to rectangles, so a group's rectangular clip maps directly.
pub struct MermaidView {
    scene: Rc<Scene>,
    viewport: Option<SceneViewport>,
    /// The last painted element bounds, in window pixels, for turning event positions into
    /// element-local pixels and for fitting.
    bounds: Option<Bounds<Pixels>>,
    fit_pending: bool,
    fit_padding: f32,
    press: Option<Press>,
    hover: Option<SceneHit>,
    selection: Option<SceneHit>,
    background: Option<gpui::Rgba>,
    images: Option<ImageResolver>,
}

impl EventEmitter<MermaidEvent> for MermaidView {}

impl MermaidView {
    /// A view of `scene`, fitted to its view box the first time it paints.
    pub fn new(scene: Scene) -> Self {
        Self {
            scene: Rc::new(scene),
            viewport: None,
            bounds: None,
            fit_pending: false,
            fit_padding: DEFAULT_FIT_PADDING,
            press: None,
            hover: None,
            selection: None,
            background: None,
            images: None,
        }
    }

    /// Replaces the drawing and fits it again.
    pub fn set_scene(&mut self, scene: Scene, cx: &mut Context<Self>) {
        self.scene = Rc::new(scene);
        self.selection = None;
        self.hover = None;
        self.fit_pending = true;
        cx.notify();
    }

    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// The backing color around the diagram; `color_surface` by default.
    pub fn background(mut self, color: gpui::Rgba) -> Self {
        self.background = Some(color);
        self
    }

    /// How much room an automatic fit leaves around the view box.
    pub fn fit_padding(mut self, padding: f32) -> Self {
        self.fit_padding = padding.max(0.0);
        self
    }

    /// Resolves the scene's named images; without one, images are left unpainted.
    pub fn images(mut self, resolve: impl Fn(&str) -> Option<Arc<RenderImage>> + 'static) -> Self {
        self.images = Some(Arc::new(resolve));
        self
    }

    /// Fits the scene's view box in the element on the next paint.
    pub fn fit(&mut self, cx: &mut Context<Self>) {
        self.fit_pending = true;
        cx.notify();
    }

    /// Zooms around the middle of the element.
    pub fn zoom_to(&mut self, zoom: f32, cx: &mut Context<Self>) {
        let Some(bounds) = self.bounds else {
            return;
        };
        self.zoom_at(
            point(bounds.size.width / 2.0, bounds.size.height / 2.0),
            zoom,
            cx,
        );
    }

    /// Zooms around an element-local pixel. An app that receives a pinch gesture GPUI 0.2.2
    /// does not surface can drive the same zoom the secondary-modifier scroll does.
    pub fn zoom_at(&mut self, focus: Point<Pixels>, zoom: f32, cx: &mut Context<Self>) {
        let Some(viewport) = self.viewport else {
            return;
        };
        self.viewport = Some(viewport.zoom_at(focus, zoom));
        cx.notify();
    }

    pub fn viewport(&self) -> Option<SceneViewport> {
        self.viewport
    }

    pub fn selection(&self) -> Option<&SceneHit> {
        self.selection.as_ref()
    }

    pub fn hover(&self) -> Option<&SceneHit> {
        self.hover.as_ref()
    }

    fn ensure_viewport(&mut self, bounds: Bounds<Pixels>) {
        if self.viewport.is_none() || self.fit_pending {
            self.viewport = Some(SceneViewport::fit(
                self.scene.view_box,
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
                self.fit_padding,
            ));
            self.fit_pending = false;
        }
    }

    fn local(&self, position: Point<Pixels>) -> Option<Point<Pixels>> {
        let bounds = self.bounds?;
        Some(point(
            position.x - bounds.origin.x,
            position.y - bounds.origin.y,
        ))
    }

    fn hit_at(&self, local: Point<Pixels>) -> Option<SceneHit> {
        let viewport = self.viewport?;
        hit_test(
            self.scene.as_ref(),
            viewport.to_scene(local),
            HIT_TOLERANCE / viewport.zoom as f64,
        )
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.click_count == 2 {
            self.fit(cx);
            return;
        }
        let Some(local) = self.local(event.position) else {
            return;
        };
        let Some(viewport) = self.viewport else {
            return;
        };
        let hit = self.hit_at(local);
        self.press = Some(Press {
            origin: local,
            viewport,
            panning: hit.is_none(),
        });
        if self.hover != hit {
            self.hover = hit.clone();
            cx.emit(MermaidEvent::Hovered(hit.clone()));
        }
        if self.selection != hit {
            self.selection = hit.clone();
            cx.emit(MermaidEvent::Selected(hit));
        }
        cx.notify();
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.press.is_some_and(|press| press.panning) {
            self.drag(event.position, cx);
            return;
        }
        let Some(local) = self.local(event.position) else {
            return;
        };
        let hit = self.hit_at(local);
        if self.hover != hit {
            self.hover = hit.clone();
            cx.emit(MermaidEvent::Hovered(hit));
            cx.notify();
        }
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.press.take().is_some() {
            cx.notify();
        }
    }

    fn scroll(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(local) = self.local(event.position) else {
            return;
        };
        let Some(viewport) = self.viewport else {
            return;
        };
        let (dx, dy) = match event.delta {
            ScrollDelta::Pixels(delta) => (f32::from(delta.x), f32::from(delta.y)),
            ScrollDelta::Lines(delta) => (delta.x * LINE_HEIGHT, delta.y * LINE_HEIGHT),
        };
        if event.modifiers.secondary() || event.modifiers.control {
            let zoom = viewport.zoom * (-dy * 0.0025).exp();
            self.viewport = Some(viewport.zoom_at(local, zoom));
        } else if dx != 0.0 || dy != 0.0 {
            self.viewport = Some(viewport.pan_by(-dx, -dy));
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn drag(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(press) = self.press else {
            return;
        };
        if !press.panning {
            return;
        }
        let Some(local) = self.local(position) else {
            return;
        };
        let dx = f32::from(local.x - press.origin.x);
        let dy = f32::from(local.y - press.origin.y);
        self.viewport = Some(press.viewport.pan_by(dx, dy));
        cx.notify();
    }
}

impl Render for MermaidView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let background = self.background.unwrap_or(theme.color_surface);
        let images = self.images.clone();
        let entity: Entity<MermaidView> = cx.entity();

        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(background)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_scroll_wheel(cx.listener(Self::scroll))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        entity.update(cx, |view, _| {
                            view.bounds = Some(bounds);
                            view.ensure_viewport(bounds);
                        });
                        let (scene, viewport, selection, hover, panning) = {
                            let view = entity.read(cx);
                            (
                                view.scene.clone(),
                                view.viewport.unwrap_or_default(),
                                view.selection.clone(),
                                view.hover.is_some(),
                                view.press.is_some_and(|press| press.panning),
                            )
                        };
                        window.set_window_cursor_style(if panning {
                            CursorStyle::ClosedHand
                        } else if hover {
                            CursorStyle::PointingHand
                        } else {
                            CursorStyle::OpenHand
                        });
                        paint_scene(&scene, bounds, viewport, theme, images.as_ref(), window, cx);
                        if let Some(hit) = &selection {
                            paint_selection(hit, bounds, viewport, theme, window);
                        }
                        if panning {
                            let moves = entity.clone();
                            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                                if phase != DispatchPhase::Capture {
                                    return;
                                }
                                moves.update(cx, |view, cx| view.drag(event.position, cx));
                                cx.stop_propagation();
                            });
                            let ups = entity.clone();
                            window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                                if phase != DispatchPhase::Capture
                                    || event.button != MouseButton::Left
                                {
                                    return;
                                }
                                ups.update(cx, |view, cx| {
                                    if view.press.take().is_some() {
                                        cx.notify();
                                    }
                                });
                                cx.stop_propagation();
                            });
                        }
                    },
                )
                .size_full(),
            )
    }
}
