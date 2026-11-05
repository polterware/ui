//! The canvas editor's pieces (`canvas.docs.tsx`, `shapes.docs.tsx`): every drawing shape and
//! component family in the lit outline, the dotted well with two connected boxes, the
//! floating tool dock, the resizable side panel, and the number stepper.

use gpui::{AnyElement, App, Context, IntoElement, SharedString, Window, div, prelude::*, px};
use polterware_ui::canvas::{
    Box2, EdgePaint, FAMILIES, Line, SHAPES, ShapePaint, Viewport, canvas_dock, dock_group,
    dock_separator, dot_grid, edges_layer, route, shape_surface,
};
use polterware_ui::patterns::{PanelSide, ResizablePanel};
use polterware_ui::theme::Theme;
use polterware_ui::{Button, ButtonKind, ButtonSize, Icon, NumberStepper};

use super::super::Example;

/// The tile a drawing sits in, and the margin around it.
const TILE: (f32, f32) = (112.0, 64.0);
const PAD: f32 = 8.0;

/// An outline in the default style: the surface face under the lit rim.
fn lit(
    path: String,
    size: (f32, f32),
    marks: Option<String>,
    dashed: bool,
    theme: &Theme,
) -> ShapePaint {
    ShapePaint {
        path,
        size,
        marks,
        face: Some(theme.color_surface),
        line: if dashed {
            Line::Color(theme.tone_neutral.rim)
        } else {
            Line::Lit {
                top: theme.material_rim_strong,
                rest: theme.material_highlight_strong,
            }
        },
        width: 1.0,
        dashed,
        mark_color: theme.tone_neutral.rim,
        drop: (!dashed).then_some(theme.material_drop),
    }
}

/// `paint` fitted into a tile, its name under it.
fn tile(name: &'static str, paint: ShapePaint, theme: &Theme) -> AnyElement {
    let (w, h) = paint.size;
    let zoom = ((TILE.0 - PAD * 2.0) / w).min((TILE.1 - PAD * 2.0) / h);
    let (x, y) = ((TILE.0 - w * zoom) / 2.0, (TILE.1 - h * zoom) / 2.0);
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(4.0))
        .child(
            div().relative().w(px(TILE.0)).h(px(TILE.1)).child(
                div()
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(w * zoom))
                    .h(px(h * zoom))
                    .child(shape_surface(paint, zoom)),
            ),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(theme.color_text_secondary)
                .child(name),
        )
        .into_any_element()
}

fn shapes(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    let tiles = SHAPES.iter().map(|shape| {
        let (w, h) = shape.geometry.unwrap_or((180.0, 100.0));
        let mut paint = lit((shape.path)(w, h), (w, h), None, false, &theme);
        if shape.stroke_only {
            paint.face = None;
            paint.line = Line::Color(theme.tone_neutral.rim);
            paint.drop = None;
        }
        tile(shape.label, paint, &theme)
    });
    vec![
        div()
            .flex()
            .flex_wrap()
            .gap(px(8.0))
            .children(tiles)
            .into_any_element(),
    ]
}

fn families(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    let tiles = FAMILIES.iter().map(|family| {
        let (w, h) = family.geometry;
        let marks = family.marks.map(|marks| marks(w, h));
        tile(
            family.label,
            lit((family.path)(w, h), (w, h), marks, family.dashed, &theme),
            &theme,
        )
    });
    vec![
        div()
            .flex()
            .flex_wrap()
            .gap(px(8.0))
            .children(tiles)
            .into_any_element(),
    ]
}

/// Two boxes on the dotted well, joined by an arrowed connection and a dashed one.
fn well(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    let viewport = Viewport {
        x: 24.0,
        y: 24.0,
        zoom: 1.0,
    };
    let boxes = [
        Box2::new(0.0, 40.0, 160.0, 72.0),
        Box2::new(300.0, 0.0, 160.0, 72.0),
        Box2::new(300.0, 120.0, 160.0, 72.0),
    ];
    let node = |at: Box2| {
        div()
            .absolute()
            .left(px(at.x + viewport.x))
            .top(px(at.y + viewport.y))
            .w(px(at.width))
            .h(px(at.height))
            .child(shape_surface(
                lit(
                    polterware_ui::canvas::shapes::rounded(at.width, at.height),
                    (at.width, at.height),
                    None,
                    false,
                    &theme,
                ),
                1.0,
            ))
    };
    let edges = vec![
        EdgePaint {
            points: route(boxes[0], boxes[1]),
            color: theme.color_text_muted,
            dashed: false,
            arrow: true,
        },
        EdgePaint {
            points: route(boxes[0], boxes[2]),
            color: theme.color_info,
            dashed: true,
            arrow: true,
        },
    ];
    vec![
        div()
            .relative()
            .w_full()
            .h(px(240.0))
            .overflow_hidden()
            .rounded(px(theme.radius_control))
            .bg(theme.color_app_background)
            .child(dot_grid(viewport, 1.0, theme.color_border_default))
            .child(edges_layer(edges, viewport))
            .children(boxes.into_iter().map(node))
            .into_any_element(),
    ]
}

fn dock(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let tool = |id: &'static str, icon: Icon, label: &'static str, selected: bool| {
        Button::new(id, ButtonKind::Ghost)
            .size(ButtonSize::Icon)
            .icon(icon)
            .icon_size(14.0)
            .tooltip(label)
            .selected(selected)
    };
    vec![
        canvas_dock(cx)
            .child(
                dock_group()
                    .child(tool("dock-select", Icon::MousePointer2, "Select", true))
                    .child(tool("dock-pan", Icon::Hand, "Pan", false))
                    .child(tool("dock-connect", Icon::ConnectIcon, "Connect", false)),
            )
            .child(dock_separator(cx))
            .child(
                dock_group()
                    .child(tool("dock-undo", Icon::Undo2, "Undo", false))
                    .child(tool("dock-redo", Icon::Redo2, "Redo", false))
                    .child(dock_separator(cx))
                    .child(tool("dock-out", Icon::MinusSignIcon, "Zoom out", false))
                    .child(polterware_ui::canvas::zoom_level(1.0, cx))
                    .child(tool("dock-in", Icon::Plus, "Zoom in", false)),
            )
            .into_any_element(),
    ]
}

/// A 192px panel (168 to 256) beside the main area; drag its edge.
fn panel(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    let panel = window.use_keyed_state("canvas-panel", cx, |_, _: &mut Context<ResizablePanel>| {
        ResizablePanel::new(PanelSide::Left, 192.0, 168.0, 256.0)
    });
    let label = |text: &'static str| {
        div()
            .p(px(12.0))
            .text_size(px(13.0))
            .text_color(theme.color_text_secondary)
            .child(text)
            .into_any_element()
    };
    let side = div()
        .size_full()
        .bg(theme.color_surface)
        .border_r_1()
        .border_color(theme.color_border_default)
        .child(label("Shape library"))
        .into_any_element();
    let main = div()
        .size_full()
        .bg(theme.color_app_background)
        .child(label("Canvas"))
        .into_any_element();
    let laid = panel.update(cx, |state, cx| state.render(&panel, side, main, window, cx));
    vec![
        div()
            .w_full()
            .h(px(200.0))
            .overflow_hidden()
            .rounded(px(theme.radius_control))
            .child(laid)
            .into_any_element(),
    ]
}

fn stepper(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let mut field = |id: &'static str, label: &'static str, value: f64, min: Option<f64>| {
        let stepper = window.use_keyed_state(
            SharedString::from(id),
            cx,
            move |window, cx: &mut Context<NumberStepper>| {
                let mut stepper = NumberStepper::new(id, cx).label(label);
                if let Some(min) = min {
                    stepper = stepper.min(min);
                }
                stepper.set_value(value, window, cx);
                stepper
            },
        );
        div().w(px(120.0)).child(stepper).into_any_element()
    };
    vec![
        field("stepper-x", "X", 240.0, None),
        field("stepper-width", "Width", 180.0, Some(40.0)),
    ]
}

pub fn examples() -> Vec<Example> {
    vec![
        Example {
            slug: "canvas-shapes",
            title: "Drawing shapes",
            height: 260.0,
            render: shapes,
        },
        Example {
            slug: "canvas-families",
            title: "Component families",
            height: 260.0,
            render: families,
        },
        Example {
            slug: "canvas-well",
            title: "The dotted well with connections",
            height: 260.0,
            render: well,
        },
        Example {
            slug: "canvas-dock",
            title: "Tool dock",
            height: 80.0,
            render: dock,
        },
        Example {
            slug: "canvas-panel",
            title: "Resizable side panel",
            height: 220.0,
            render: panel,
        },
        Example {
            slug: "number-stepper",
            title: "Number stepper",
            height: 100.0,
            render: stepper,
        },
    ]
}
