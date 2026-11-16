//! The view that shows a diagram: a scene, a viewport, and the pointer.
//!
//! It behaves like gpui-kit's `Canvas`/`Plot` views for navigation: drag empty space to pan,
//! scroll to pan, the secondary modifier (command on macOS, control elsewhere) with a scroll to
//! zoom around the pointer, a double-click to fit the view box, and the hovered or selected
//! element highlighted. GPUI 0.2.2 has no magnify gesture event, so a trackpad pinch arrives as
//! its scroll-with-secondary-modifier form; that is the zoom gesture here. Selecting a node or
//! edge emits [`MermaidEvent::Selected`] with the [`SceneHit`], so the app can turn a click back
//! into an edit of the diagram source.
//!
//! A press on a node and a move past a small threshold drags that node: the view shifts its
//! drawing and the edge ends that touch it, emits [`MermaidEvent::NodeDragged`] while moving and
//! [`MermaidEvent::NodeDropped`] with the node's new place on release. The app turns the drop
//! into a pin in the canvas layout. A press on empty space still pans.

use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    Bounds, Context, CursorStyle, DispatchPhase, Entity, EventEmitter, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Render, RenderImage, ScrollDelta,
    ScrollWheelEvent, Window, canvas, div, point, prelude::*,
};
use polterware_mermaid::scene::{
    Item, PathCommand, Point as ScenePoint, Rect, Scene, SourceKind, Transform,
};

use crate::mermaid::geometry::{PathGeometry, transformed_bounds};
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
/// How far, in screen pixels, a press on a node moves before it becomes a drag.
const DRAG_THRESHOLD: f32 = 3.0;
/// How near an edge end has to be to a node's box to travel with it.
const END_TOLERANCE: f64 = 1.0;

/// What a [`MermaidView`] tells its app.
#[derive(Clone, Debug, PartialEq)]
pub enum MermaidEvent {
    /// The element under a click, or `None` when the click landed on empty space.
    Selected(Option<SceneHit>),
    /// The element under the pointer, when it changes.
    Hovered(Option<SceneHit>),
    /// A node is being dragged: `delta` is how far it moved, in scene coordinates.
    NodeDragged { id: String, delta: ScenePoint },
    /// A dragged node was released at `position`, its box's top-left in scene coordinates.
    NodeDropped { id: String, position: ScenePoint },
}

#[derive(Clone, Copy, Debug)]
struct Press {
    origin: Point<Pixels>,
    viewport: SceneViewport,
    panning: bool,
}

/// A node press that becomes a drag once the pointer moves past its threshold.
#[derive(Clone, Debug)]
struct NodeDrag {
    id: String,
    press: ScenePoint,
    bounds: Rect,
    last: ScenePoint,
    threshold: f64,
    moved: bool,
}

impl NodeDrag {
    fn new(id: String, press: ScenePoint, bounds: Rect, threshold: f64) -> Self {
        Self {
            id,
            press,
            bounds,
            last: press,
            threshold,
            moved: false,
        }
    }

    fn delta(&self, at: ScenePoint) -> ScenePoint {
        ScenePoint::new(at.x - self.press.x, at.y - self.press.y)
    }

    /// The drag's total delta once the pointer has moved past the threshold.
    fn update(&mut self, at: ScenePoint) -> Option<(String, ScenePoint)> {
        self.last = at;
        let delta = self.delta(at);
        if !self.moved && delta.x.hypot(delta.y) < self.threshold {
            return None;
        }
        self.moved = true;
        Some((self.id.clone(), delta))
    }

    /// The node's new top-left, when it moved at all.
    fn finish(&mut self, at: ScenePoint) -> Option<(String, ScenePoint)> {
        self.last = at;
        if !self.moved {
            return None;
        }
        let delta = self.delta(at);
        Some((
            self.id.clone(),
            ScenePoint::new(self.bounds.x + delta.x, self.bounds.y + delta.y),
        ))
    }
}

/// A diagram in a GPUI view: pan, zoom, fit, and hit testing over a
/// [`Scene`](polterware_mermaid::scene::Scene).
///
/// The look is the Mermaid SVG (the upstream renderers' shapes and labels), and the behavior is
/// gpui-kit's canvas views: drag to pan, scroll to pan, secondary-modifier scroll to zoom around
/// the pointer, and a double-click to fit. The scene is painted by GPUI paths, quads, text, and
/// images; GPUI 0.2.2 clips content to rectangles, so a group's rectangular clip maps directly.
/// A grid dot's radius on screen, at any zoom.
const DOT_RADIUS: f32 = 1.0;

pub struct MermaidView {
    scene: Rc<Scene>,
    viewport: Option<SceneViewport>,
    /// The last painted element bounds, in window pixels, for turning event positions into
    /// element-local pixels and for fitting.
    bounds: Option<Bounds<Pixels>>,
    fit_pending: bool,
    fit_padding: f32,
    press: Option<Press>,
    /// The node under a press, once it drags.
    drag: Option<NodeDrag>,
    /// The drawing before the drag, and the one the pointer has moved.
    drag_base: Option<Rc<Scene>>,
    drag_scene: Option<Rc<Scene>>,
    /// The dragged node and its delta, so its selection frame follows it.
    drag_delta: Option<(String, ScenePoint)>,
    hover: Option<SceneHit>,
    selection: Option<SceneHit>,
    background: Option<gpui::Rgba>,
    /// The dotted grid under the drawing, rounded off at the face's corners.
    dots: Option<f32>,
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
            drag: None,
            drag_base: None,
            drag_scene: None,
            drag_delta: None,
            hover: None,
            selection: None,
            background: None,
            dots: None,
            images: None,
        }
    }

    /// Replaces the drawing and fits it again.
    pub fn set_scene(&mut self, scene: Scene, cx: &mut Context<Self>) {
        self.scene = Rc::new(scene);
        self.reselect();
        self.hover = None;
        self.press = None;
        self.end_drag();
        self.fit_pending = true;
        cx.notify();
    }

    /// Replaces the drawing, keeping the pan and zoom it was left at. A source edit redraws
    /// this way, so typing a label does not send the diagram back to its fitted view.
    pub fn update_scene(&mut self, scene: Scene, cx: &mut Context<Self>) {
        self.scene = Rc::new(scene);
        self.reselect();
        self.hover = None;
        self.press = None;
        self.end_drag();
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

    /// Lays the editor's dotted grid ([`paint_dot_grid`](crate::canvas::paint_dot_grid)) under
    /// the drawing in `--color-border-default`, following the pan and zoom; `corner` is the
    /// radius of the face it sits on, whose corners keep no dots.
    pub fn dot_grid(mut self, corner: f32) -> Self {
        self.dots = Some(corner);
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

    /// Selects the node or edge with source `id` from the current drawing, or clears the
    /// selection with `None`. An app calls this when the selection comes from elsewhere (an
    /// inspector, a text edit, a palette insert), so the diagram outlines what the rest of the
    /// editor shows.
    pub fn select_id(&mut self, id: Option<&str>, cx: &mut Context<Self>) {
        let hit = id.and_then(|id| source_hit(self.scene.as_ref(), id));
        if self.selection != hit {
            self.selection = hit;
            cx.notify();
        }
    }

    /// Re-resolves the selected element against the current drawing, so a redraw keeps the
    /// outline on it and drops it when the element is gone.
    fn reselect(&mut self) {
        let id = self.selection.as_ref().map(|hit| hit.source.id.clone());
        self.selection = id.and_then(|id| source_hit(self.scene.as_ref(), &id));
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

    fn mouse_down(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
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
        let at = viewport.to_scene(local);
        self.press = Some(Press {
            origin: local,
            viewport,
            panning: hit.is_none(),
        });
        if let Some(hit) = hit
            .as_ref()
            .filter(|hit| hit.source.kind == SourceKind::Node)
            && let Some(bounds) = node_bounds(self.scene.as_ref(), &hit.source.id)
        {
            self.drag = Some(NodeDrag::new(
                hit.source.id.clone(),
                at,
                bounds,
                f64::from(DRAG_THRESHOLD) / f64::from(viewport.zoom),
            ));
            self.drag_base = Some(self.scene.clone());
        }
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

    fn mouse_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if self.press.is_some_and(|press| press.panning) {
            self.drag_viewport(event.position, cx);
            return;
        }
        if self.drag.is_some() {
            self.drag_node(event.position, cx);
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

    fn mouse_up(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        self.press = None;
        if self.drag.is_some() {
            self.drop_node(event.position, cx);
            return;
        }
        cx.notify();
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
            // The wheel's delta already runs the way the fingers move: the drawing follows
            // them, as the image viewer's picture does.
            self.viewport = Some(viewport.pan_by(dx, dy));
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn drag_viewport(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
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

    /// Moves the pressed node with the pointer, once it has dragged past its threshold.
    fn drag_node(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let (Some(local), Some(viewport), Some(drag)) =
            (self.local(position), self.viewport, self.drag.as_mut())
        else {
            return;
        };
        let Some((id, delta)) = drag.update(viewport.to_scene(local)) else {
            return;
        };
        let Some(base) = self.drag_base.as_ref() else {
            return;
        };
        self.drag_scene = Some(Rc::new(moved_scene(base, &id, delta)));
        self.drag_delta = Some((id.clone(), delta));
        cx.emit(MermaidEvent::NodeDragged { id, delta });
        cx.notify();
    }

    /// Keeps the drawing the drag produced and tells the app where the node landed.
    fn drop_node(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(local) = self.local(position) else {
            self.end_drag();
            cx.notify();
            return;
        };
        let Some(viewport) = self.viewport else {
            self.end_drag();
            cx.notify();
            return;
        };
        let Some(mut drag) = self.drag.take() else {
            return;
        };
        let dropped = drag.finish(viewport.to_scene(local));
        if let Some(scene) = self.drag_scene.take() {
            self.scene = scene;
            self.reselect();
        }
        self.drag_base = None;
        self.drag_delta = None;
        if let Some((id, position)) = dropped {
            cx.emit(MermaidEvent::NodeDropped { id, position });
        }
        cx.notify();
    }

    fn end_drag(&mut self) {
        self.drag = None;
        self.drag_base = None;
        self.drag_scene = None;
        self.drag_delta = None;
    }

    fn is_dragging(&self) -> bool {
        self.drag.is_some()
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
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, event: &MouseDownEvent, _, cx| view.mouse_down(event, cx)),
            )
            .on_mouse_move(
                cx.listener(|view, event: &MouseMoveEvent, _, cx| view.mouse_move(event, cx)),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, event: &MouseUpEvent, _, cx| view.mouse_up(event, cx)),
            )
            .on_scroll_wheel(cx.listener(Self::scroll))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        entity.update(cx, |view, _| {
                            view.bounds = Some(bounds);
                            view.ensure_viewport(bounds);
                        });
                        let (scene, viewport, selection, hover, panning, dragging) = {
                            let view = entity.read(cx);
                            (
                                view.drag_scene
                                    .clone()
                                    .unwrap_or_else(|| view.scene.clone()),
                                view.viewport.unwrap_or_default(),
                                view.selection.clone(),
                                view.hover.is_some(),
                                view.press.is_some_and(|press| press.panning),
                                view.is_dragging(),
                            )
                        };
                        window.set_window_cursor_style(if panning || dragging {
                            CursorStyle::ClosedHand
                        } else if hover {
                            CursorStyle::PointingHand
                        } else {
                            CursorStyle::OpenHand
                        });
                        if let Some(corner) = entity.read(cx).dots {
                            crate::canvas::paint_dot_grid_rounded(
                                bounds,
                                crate::canvas::Viewport {
                                    x: viewport.x,
                                    y: viewport.y,
                                    zoom: viewport.zoom,
                                },
                                DOT_RADIUS,
                                theme.color_border_default,
                                corner,
                                window,
                            );
                        }
                        paint_scene(&scene, bounds, viewport, theme, images.as_ref(), window, cx);
                        if let Some(hit) = &selection {
                            let hit = match &entity.read(cx).drag_delta {
                                Some((id, delta))
                                    if hit.source.kind == SourceKind::Node
                                        && hit.source.id == *id =>
                                {
                                    let mut hit = hit.clone();
                                    hit.bounds = Rect::new(
                                        hit.bounds.x + delta.x,
                                        hit.bounds.y + delta.y,
                                        hit.bounds.width,
                                        hit.bounds.height,
                                    );
                                    hit
                                }
                                _ => hit.clone(),
                            };
                            paint_selection(&hit, bounds, viewport, theme, window);
                        }
                        if panning || dragging {
                            let moves = entity.clone();
                            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                                if phase != DispatchPhase::Capture {
                                    return;
                                }
                                moves.update(cx, |view, cx| view.mouse_move(event, cx));
                                cx.stop_propagation();
                            });
                            let ups = entity.clone();
                            window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                                if phase != DispatchPhase::Capture
                                    || event.button != MouseButton::Left
                                {
                                    return;
                                }
                                ups.update(cx, |view, cx| view.mouse_up(event, cx));
                                cx.stop_propagation();
                            });
                        }
                    },
                )
                .size_full(),
            )
    }
}

/// The hit for the element `id` draws: its source and the box its items cover. The first item
/// that names it wins, which is the element's own group for a node and its path for an edge,
/// so a label beside it does not widen the outline.
fn source_hit(scene: &Scene, id: &str) -> Option<SceneHit> {
    let mut hit = None;
    collect_source_hit(&scene.items, Transform::IDENTITY, id, &mut hit);
    hit
}

fn collect_source_hit(items: &[Item], transform: Transform, id: &str, out: &mut Option<SceneHit>) {
    for item in items {
        if out.is_some() {
            return;
        }
        let world = match item {
            Item::Group(group) => group.transform.then(transform),
            _ => transform,
        };
        if let Some(source) = item.source().filter(|source| source.id == id)
            && let Some(bounds) = item_bounds(item, transform)
        {
            *out = Some(SceneHit {
                source: source.clone(),
                bounds,
            });
            return;
        }
        if let Item::Group(group) = item {
            collect_source_hit(&group.items, world, id, out);
        }
    }
}

/// The box a node's own items cover, in scene coordinates.
fn node_bounds(scene: &Scene, id: &str) -> Option<Rect> {
    let mut bounds: Option<Rect> = None;
    collect_node_bounds(&scene.items, Transform::IDENTITY, id, &mut bounds);
    bounds
}

fn collect_node_bounds(items: &[Item], transform: Transform, id: &str, out: &mut Option<Rect>) {
    for item in items {
        let world = match item {
            Item::Group(group) => group.transform.then(transform),
            _ => transform,
        };
        if item
            .source()
            .is_some_and(|source| source.kind == SourceKind::Node && source.id == id)
            && let Some(bounds) = item_bounds(item, transform)
        {
            *out = Some(out.map_or(bounds, |current: Rect| current.union(bounds)));
        }
        if let Item::Group(group) = item {
            collect_node_bounds(&group.items, world, id, out);
        }
    }
}

/// The drawing with node `id` moved by `delta`, its edge ends following it.
fn moved_scene(scene: &Scene, id: &str, delta: ScenePoint) -> Scene {
    let mut next = scene.clone();
    let Some(bounds) = node_bounds(scene, id) else {
        return next;
    };
    shift_nodes(&mut next.items, id, delta);
    shift_edge_ends(&mut next.items, bounds, delta);
    next
}

/// Moves the node's group, or its own items when it has no group of its own.
fn shift_nodes(items: &mut [Item], id: &str, delta: ScenePoint) {
    for item in items {
        if item
            .source()
            .is_some_and(|source| source.kind == SourceKind::Node && source.id == id)
        {
            match item {
                Item::Group(group) => {
                    group.transform.e += delta.x;
                    group.transform.f += delta.y;
                    continue;
                }
                other => {
                    translate_item(other, delta);
                    continue;
                }
            }
        }
        if let Item::Group(group) = item {
            shift_nodes(&mut group.items, id, delta);
        }
    }
}

/// Moves the ends of every edge that sit on the node's box, as straight lines.
fn shift_edge_ends(items: &mut [Item], bounds: Rect, delta: ScenePoint) {
    for item in items {
        match item {
            Item::Path(path) => {
                if !path
                    .source
                    .as_ref()
                    .is_some_and(|source| source.kind == SourceKind::Edge)
                {
                    continue;
                }
                let Some(first) = path.path.commands.first_mut() else {
                    continue;
                };
                if command_end(first).is_some_and(|point| touches(point, bounds)) {
                    shift_end(first, delta);
                }
                let Some(last) = path
                    .path
                    .commands
                    .iter_mut()
                    .rev()
                    .find(|command| command_end(command).is_some())
                else {
                    continue;
                };
                if command_end(last).is_some_and(|point| touches(point, bounds)) {
                    shift_end(last, delta);
                }
            }
            Item::Group(group) => shift_edge_ends(&mut group.items, bounds, delta),
            _ => {}
        }
    }
}

fn touches(point: ScenePoint, rect: Rect) -> bool {
    let dx = (rect.x - point.x).max(point.x - rect.right()).max(0.0);
    let dy = (rect.y - point.y).max(point.y - rect.bottom()).max(0.0);
    dx.hypot(dy) <= END_TOLERANCE
}

fn command_end(command: &PathCommand) -> Option<ScenePoint> {
    match *command {
        PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => Some(ScenePoint::new(x, y)),
        PathCommand::CubicTo { x, y, .. }
        | PathCommand::QuadTo { x, y, .. }
        | PathCommand::ArcTo { x, y, .. } => Some(ScenePoint::new(x, y)),
        PathCommand::Close => None,
    }
}

fn shift_end(command: &mut PathCommand, delta: ScenePoint) {
    let (x, y) = match command {
        PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => (x, y),
        PathCommand::CubicTo { x, y, .. }
        | PathCommand::QuadTo { x, y, .. }
        | PathCommand::ArcTo { x, y, .. } => (x, y),
        PathCommand::Close => return,
    };
    *x += delta.x;
    *y += delta.y;
}

fn translate_item(item: &mut Item, delta: ScenePoint) {
    match item {
        Item::Group(group) => {
            group.transform.e += delta.x;
            group.transform.f += delta.y;
        }
        Item::Rect(rect) => {
            rect.rect.x += delta.x;
            rect.rect.y += delta.y;
        }
        Item::Ellipse(ellipse) => {
            ellipse.center.x += delta.x;
            ellipse.center.y += delta.y;
        }
        Item::Text(text) => {
            text.position.x += delta.x;
            text.position.y += delta.y;
        }
        Item::Image(image) => {
            image.rect.x += delta.x;
            image.rect.y += delta.y;
        }
        Item::Path(path) => {
            for command in &mut path.path.commands {
                match command {
                    PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => {
                        *x += delta.x;
                        *y += delta.y;
                    }
                    PathCommand::CubicTo {
                        c1x,
                        c1y,
                        c2x,
                        c2y,
                        x,
                        y,
                    } => {
                        *c1x += delta.x;
                        *c1y += delta.y;
                        *c2x += delta.x;
                        *c2y += delta.y;
                        *x += delta.x;
                        *y += delta.y;
                    }
                    PathCommand::QuadTo { cx, cy, x, y } => {
                        *cx += delta.x;
                        *cy += delta.y;
                        *x += delta.x;
                        *y += delta.y;
                    }
                    PathCommand::ArcTo { x, y, .. } => {
                        *x += delta.x;
                        *y += delta.y;
                    }
                    PathCommand::Close => {}
                }
            }
        }
    }
}

fn item_bounds(item: &Item, transform: Transform) -> Option<Rect> {
    match item {
        Item::Group(group) => {
            let world = group.transform.then(transform);
            group
                .items
                .iter()
                .filter_map(|item| item_bounds(item, world))
                .reduce(Rect::union)
        }
        Item::Path(path) => PathGeometry::from_path(&path.path)
            .bounds()
            .map(|bounds| transformed_bounds(bounds, transform)),
        Item::Rect(rect) => Some(transformed_bounds(rect.rect, transform)),
        Item::Ellipse(ellipse) => Some(transformed_bounds(
            Rect::new(
                ellipse.center.x - ellipse.radius_x,
                ellipse.center.y - ellipse.radius_y,
                ellipse.radius_x * 2.0,
                ellipse.radius_y * 2.0,
            ),
            transform,
        )),
        Item::Text(text) => Some(transformed_bounds(text.bounds(), transform)),
        Item::Image(image) => Some(transformed_bounds(image.rect, transform)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polterware_mermaid::scene::{
        Group, Path, PathShape, PathStyle, RectShape, SourceRef, TextBlock, TextRun,
    };

    fn node_item(id: &str, rect: Rect) -> Item {
        let mut group = Group::translated(rect.x, rect.y).with_source(SourceRef::node(id));
        group.push(
            RectShape::new(
                Rect::new(0.0, 0.0, rect.width, rect.height),
                4.0,
                PathStyle::filled("#ECECFF"),
            )
            .with_source(SourceRef::node(id)),
        );
        Item::Group(group)
    }

    fn edge_item(from: ScenePoint, to: ScenePoint) -> Item {
        let mut path = Path::new();
        path.move_to(from).line_to(to);
        Item::Path(
            PathShape::new(path, PathStyle::stroked("#333333", 2.0))
                .with_source(SourceRef::edge("e1")),
        )
    }

    fn scene() -> Scene {
        let mut scene = Scene::default();
        scene.push(edge_item(
            ScenePoint::new(100.0, 20.0),
            ScenePoint::new(220.0, 20.0),
        ));
        scene.push(node_item("a", Rect::new(0.0, 0.0, 100.0, 40.0)));
        scene.push(node_item("b", Rect::new(220.0, 0.0, 100.0, 40.0)));
        scene
    }

    #[test]
    fn a_press_past_the_threshold_drags_a_node() {
        let mut drag = NodeDrag::new(
            "a".into(),
            ScenePoint::new(10.0, 10.0),
            Rect::new(0.0, 0.0, 100.0, 40.0),
            3.0,
        );
        assert!(drag.update(ScenePoint::new(12.0, 10.0)).is_none());
        let (id, delta) = drag.update(ScenePoint::new(20.0, 15.0)).expect("a drag");
        assert_eq!(id, "a");
        assert_eq!(delta, ScenePoint::new(10.0, 5.0));
        let (id, position) = drag.finish(ScenePoint::new(30.0, 25.0)).expect("a drop");
        assert_eq!(id, "a");
        assert_eq!(position, ScenePoint::new(20.0, 15.0));
    }

    #[test]
    fn a_press_that_does_not_move_drops_nothing() {
        let mut drag = NodeDrag::new(
            "a".into(),
            ScenePoint::new(10.0, 10.0),
            Rect::new(0.0, 0.0, 100.0, 40.0),
            3.0,
        );
        assert!(drag.update(ScenePoint::new(11.0, 11.0)).is_none());
        assert!(drag.finish(ScenePoint::new(11.0, 11.0)).is_none());
    }

    #[test]
    fn dragging_moves_the_node_and_the_edge_end_that_touches_it() {
        let scene = scene();
        let moved = moved_scene(&scene, "a", ScenePoint::new(40.0, 30.0));
        let bounds = node_bounds(&moved, "a").expect("the moved node");
        assert_eq!(bounds, Rect::new(40.0, 30.0, 100.0, 40.0));
        let edge = moved
            .items
            .iter()
            .find_map(|item| match item {
                Item::Path(path) => Some(path),
                _ => None,
            })
            .expect("the edge");
        assert_eq!(
            command_end(edge.path.commands.first().expect("a start")),
            Some(ScenePoint::new(140.0, 50.0))
        );
        assert_eq!(
            command_end(edge.path.commands.last().expect("an end")),
            Some(ScenePoint::new(220.0, 20.0))
        );
        assert_eq!(
            node_bounds(&moved, "b"),
            Some(Rect::new(220.0, 0.0, 100.0, 40.0))
        );
    }

    #[test]
    fn a_node_label_travels_with_its_node() {
        let block = TextBlock::new(
            Vec::new(),
            polterware_mermaid::scene::Size::new(40.0, 12.0),
            13.0,
        );
        let text = TextRun::centered(ScenePoint::new(50.0, 20.0), Default::default(), block)
            .with_source(SourceRef::node("a"));
        let mut scene = Scene::default();
        scene.push(node_item("a", Rect::new(0.0, 0.0, 100.0, 40.0)));
        scene.push(Item::Text(text));
        let moved = moved_scene(&scene, "a", ScenePoint::new(10.0, 5.0));
        let label = moved
            .items
            .iter()
            .find_map(|item| match item {
                Item::Text(text) => Some(text),
                _ => None,
            })
            .expect("the label");
        assert_eq!(label.position, ScenePoint::new(60.0, 25.0));
    }

    #[test]
    fn a_selection_by_source_id_finds_the_node_and_its_box() {
        let scene = scene();
        let hit = source_hit(&scene, "a").expect("the node");
        assert_eq!(hit.source, SourceRef::node("a"));
        assert_eq!(hit.bounds, Rect::new(0.0, 0.0, 100.0, 40.0));
        let edge = source_hit(&scene, "e1").expect("the edge");
        assert_eq!(edge.source, SourceRef::edge("e1"));
        assert_eq!(edge.bounds, Rect::new(100.0, 20.0, 120.0, 0.0));
        assert!(source_hit(&scene, "gone").is_none());
    }
}
