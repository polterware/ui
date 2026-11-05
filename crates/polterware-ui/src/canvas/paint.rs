//! How the canvas paints (`node-renderer.tsx`, `canvas.css`).
//!
//! - [`shape_surface`]: an outline as a face just above the background with one thin line,
//!   details included (`ShapeSurface`). A chosen fill, line color, width, or pattern replaces
//!   those; the line keeps its width on screen whatever the zoom (`vector-effect:
//!   non-scaling-stroke`). The default line is the shared lit material: a dark drop under the
//!   face and a 0.5px rim, brightest along the top 24 units and fading to the faint side and
//!   bottom rim. GPUI has no blur for a path, so the drop is the face twice more, 1px and 2px
//!   lower, in the drop color.
//! - [`sticky_surface`]: a borderless face in the note's color with its bottom-right corner
//!   folded up, creased in the note's ink (`StickyNoteSurface`).
//! - [`selection_frame`]: a rounded info rim 3 units outside the node, 1.5px on screen.
//! - [`edges_layer`]: the connections, 2 units wide (they scale with the zoom, as the
//!   Electron editor's do), dashed `6 4` where the kind says, with an arrow at the target
//!   (`markerWidth="7"` on a 10-unit view box: a 14-unit triangle whose tip passes the end
//!   by 1.4).
//! - [`dot_grid`]: the dotted well (`.pw-canvas-surface`): a 0.75px dot with a soft edge in
//!   `--color-border-default` every 24 units, following the viewport.

use gpui::{
    App, Bounds, Hsla, IntoElement, Pixels, Rgba, Window, canvas, fill, linear_color_stop,
    linear_gradient, point, prelude::*, px, size,
};

use super::geometry::{Pt, Viewport};
use super::path::{Paint, Placement, Segment, build, closed_only, parse};

/// How a shape's line is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Line {
    /// The lit material's rim: a 0.5px gradient from `--material-rim-strong` on top to
    /// `--material-highlight-strong`.
    Lit {
        top: Rgba,
        rest: Rgba,
    },
    Color(Rgba),
    None,
}

/// One outline as the canvas paints it.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapePaint {
    /// The outline in the node's w×h box, in document units.
    pub path: String,
    /// The node's size in document units.
    pub size: (f32, f32),
    /// Small solid marks painted in the line color (a window's dots).
    pub marks: Option<String>,
    /// The face; `None` draws none (a stroke-only drawing, or the `none` fill).
    pub face: Option<Rgba>,
    pub line: Line,
    /// The line's width on screen, 1 to 3.
    pub width: f32,
    pub dashed: bool,
    /// What the marks are painted in.
    pub mark_color: Rgba,
    /// The lit material's drop under the face.
    pub drop: Option<Rgba>,
}

/// The `6 4` dash of a dashed line, in screen pixels.
const DASH: [f32; 2] = [6.0, 4.0];

/// Paints `shape` with its box's origin at `origin`, `zoom` screen pixels per unit.
pub fn paint_shape(
    shape: &ShapePaint,
    origin: gpui::Point<Pixels>,
    zoom: f32,
    window: &mut Window,
) {
    let segments = parse(&shape.path);
    let placement = Placement::new(origin, zoom);
    if let Some(face) = shape.face {
        let outline = closed_only(&segments);
        if let Some(drop) = shape.drop {
            for (lift, alpha) in [(2.0, 0.5), (1.0, 0.7)] {
                let lower = Placement::new(point(origin.x, origin.y + px(lift)), zoom);
                if let Some(path) = build(&outline, lower, &Paint::Fill) {
                    window.paint_path(
                        path,
                        Rgba {
                            a: drop.a * alpha,
                            ..drop
                        },
                    );
                }
            }
        }
        if let Some(path) = build(&outline, placement, &Paint::Fill) {
            window.paint_path(path, face);
        }
    }
    let dash = shape.dashed.then(|| DASH.to_vec());
    match shape.line {
        Line::None => {}
        Line::Color(color) => {
            let paint = Paint::Stroke {
                width: shape.width,
                dash,
            };
            if let Some(path) = build(&segments, placement, &paint) {
                window.paint_path(path, color);
            }
        }
        Line::Lit { top, rest } => {
            let paint = Paint::Stroke { width: 0.5, dash };
            if let Some(path) = build(&segments, placement, &paint) {
                // userSpaceOnUse from 0 to min(24, h): the stops sit on the box's height.
                let height = shape.size.1.max(1.0);
                let stop = (24.0_f32.min(height) / height).clamp(0.0, 1.0);
                window.paint_path(
                    path,
                    linear_gradient(
                        180.0,
                        linear_color_stop(top, 0.0),
                        linear_color_stop(rest, stop),
                    ),
                );
            }
        }
    }
    if let Some(marks) = &shape.marks
        && let Some(path) = build(&parse(marks), placement, &Paint::Fill)
    {
        window.paint_path(path, shape.mark_color);
    }
}

/// An element filling its parent (the node's screen box) that paints `shape` at `zoom`.
pub fn shape_surface(shape: ShapePaint, zoom: f32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window, _| {
            paint_shape(&shape, bounds.origin, zoom, window)
        },
    )
    .absolute()
    .inset_0()
}

/// The sticky note's face and fold for a `w`×`h` note.
pub fn sticky_paths(w: f32, h: f32) -> (String, String) {
    let fold = 18.0_f32.min(w / 5.0).min(h / 5.0);
    let r = 4.0_f32.min(fold / 2.0);
    let body = format!(
        "M{r} 0H{a}A{r} {r} 0 0 1 {w} {r}V{b}L{c} {h}H{r}A{r} {r} 0 0 1 0 {d}V{r}A{r} {r} 0 0 1 {r} 0Z",
        a = w - r,
        b = h - fold,
        c = w - fold,
        d = h - r
    );
    let flap = format!("M{w} {b}H{c}V{h}Z", b = h - fold, c = w - fold);
    (body, flap)
}

/// A sticky note's surface filling its parent: the face and the fold in `color`, the
/// crease in `ink`.
pub fn sticky_surface(size: (f32, f32), color: Rgba, ink: Rgba, zoom: f32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window, _| {
            let (body, flap) = sticky_paths(size.0, size.1);
            let placement = Placement::new(bounds.origin, zoom);
            if let Some(path) = build(&parse(&body), placement, &Paint::Fill) {
                window.paint_path(path, color);
            }
            let flap = parse(&flap);
            if let Some(path) = build(&flap, placement, &Paint::Fill) {
                window.paint_path(path, color);
            }
            let crease = Paint::Stroke {
                width: 1.0,
                dash: None,
            };
            if let Some(path) = build(&flap, placement, &crease) {
                window.paint_path(path, ink);
            }
        },
    )
    .absolute()
    .inset_0()
}

/// A rounded rectangle's path whose radius never passes half a side (SVG's `rx`).
fn frame_path(x: f32, y: f32, w: f32, h: f32, radius: f32) -> String {
    let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
    format!(
        "M{a} {y}H{b}A{r} {r} 0 0 1 {right} {c}V{d}A{r} {r} 0 0 1 {b} {bottom}H{a}A{r} {r} 0 0 1 {x} {d}V{c}A{r} {r} 0 0 1 {a} {y}Z",
        a = x + r,
        b = x + w - r,
        c = y + r,
        d = y + h - r,
        right = x + w,
        bottom = y + h,
    )
}

/// The selection rim around a `w`×`h` node whose box starts at `origin`.
pub fn paint_selection(
    origin: gpui::Point<Pixels>,
    size: (f32, f32),
    zoom: f32,
    color: Rgba,
    window: &mut Window,
) {
    let path = frame_path(-3.0, -3.0, size.0 + 6.0, size.1 + 6.0, 8.0);
    let paint = Paint::Stroke {
        width: 1.5,
        dash: None,
    };
    if let Some(path) = build(&parse(&path), Placement::new(origin, zoom), &paint) {
        window.paint_path(path, color);
    }
}

/// An element filling its parent (the node's screen box) that paints the selection rim.
pub fn selection_frame(size: (f32, f32), zoom: f32, color: Rgba) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window, _| {
            paint_selection(bounds.origin, size, zoom, color, window)
        },
    )
    .absolute()
    .inset_0()
}

/// One connection as the canvas paints it: its route in document units.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgePaint {
    pub points: Vec<Pt>,
    pub color: Rgba,
    pub dashed: bool,
    pub arrow: bool,
}

/// Paints the connections over a surface whose origin is the viewport's screen origin.
pub fn paint_edges(
    edges: &[EdgePaint],
    origin: gpui::Point<Pixels>,
    viewport: Viewport,
    window: &mut Window,
) {
    let zoom = viewport.zoom;
    let placement = Placement::new(
        point(origin.x + px(viewport.x), origin.y + px(viewport.y)),
        zoom,
    );
    for edge in edges {
        let Some(first) = edge.points.first() else {
            continue;
        };
        let mut segments = vec![Segment::Move(first.x, first.y)];
        segments.extend(edge.points[1..].iter().map(|at| Segment::Line(at.x, at.y)));
        let paint = Paint::Stroke {
            width: 2.0 * zoom,
            dash: edge
                .dashed
                .then(|| DASH.iter().map(|value| value * zoom).collect()),
        };
        if let Some(path) = build(&segments, placement, &paint) {
            window.paint_path(path, edge.color);
        }
        if edge.arrow
            && let [.., before, end] = edge.points.as_slice()
            && let Some(arrow) = arrow_head(*before, *end)
            && let Some(path) = build(&arrow, placement, &Paint::Fill)
        {
            window.paint_path(path, edge.color);
        }
    }
}

/// The marker at the end of a 2-unit line: a 14-unit triangle, its tip 1.4 past the end.
fn arrow_head(before: Pt, end: Pt) -> Option<Vec<Segment>> {
    let (dx, dy) = (end.x - before.x, end.y - before.y);
    let length = (dx * dx + dy * dy).sqrt();
    if length == 0.0 {
        return None;
    }
    let (ux, uy) = (dx / length, dy / length);
    let (nx, ny) = (-uy, ux);
    let tip = (end.x + ux * 1.4, end.y + uy * 1.4);
    let base = (end.x - ux * 12.6, end.y - uy * 12.6);
    Some(vec![
        Segment::Move(base.0 + nx * 7.0, base.1 + ny * 7.0),
        Segment::Line(tip.0, tip.1),
        Segment::Line(base.0 - nx * 7.0, base.1 - ny * 7.0),
        Segment::Close,
    ])
}

/// An element filling its parent that paints the connections for `viewport`.
pub fn edges_layer(edges: Vec<EdgePaint>, viewport: Viewport) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window, _| {
            paint_edges(&edges, bounds.origin, viewport, window)
        },
    )
    .absolute()
    .inset_0()
}

/// The editor grid's step at 100%, in document units.
pub const GRID: f32 = 24.0;

/// Paints the dotted grid over `bounds`: a dot of `radius` screen pixels every `GRID` units,
/// centered in each cell as a CSS radial-gradient tile is.
pub fn paint_dot_grid(
    bounds: Bounds<Pixels>,
    viewport: Viewport,
    radius: f32,
    color: impl Into<Hsla>,
    window: &mut Window,
) {
    let color: Hsla = color.into();
    let step = GRID * viewport.zoom;
    if step < 2.0 {
        return;
    }
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    let start_x = viewport.x.rem_euclid(step) - step + step / 2.0;
    let start_y = viewport.y.rem_euclid(step) - step + step / 2.0;
    let mut y = start_y;
    while y < height + step {
        let mut x = start_x;
        while x < width + step {
            let dot = Bounds::new(
                point(
                    bounds.origin.x + px(x - radius),
                    bounds.origin.y + px(y - radius),
                ),
                size(px(radius * 2.0), px(radius * 2.0)),
            );
            window.paint_quad(fill(dot, color).corner_radii(px(radius)));
            x += step;
        }
        y += step;
    }
}

/// An element filling its parent with the dotted grid for `viewport`.
pub fn dot_grid(viewport: Viewport, radius: f32, color: Rgba) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window: &mut Window, _: &mut App| {
            window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
                paint_dot_grid(bounds, viewport, radius, color, window)
            })
        },
    )
    .absolute()
    .inset_0()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arrow_tip_passes_the_end() {
        let arrow = arrow_head(Pt::new(0.0, 0.0), Pt::new(100.0, 0.0)).unwrap();
        assert_eq!(arrow[1], Segment::Line(101.4, 0.0));
        assert_eq!(arrow[0], Segment::Move(87.4, 7.0));
    }

    #[test]
    fn the_note_folds_its_bottom_right_corner() {
        let (body, flap) = sticky_paths(200.0, 160.0);
        assert!(body.starts_with("M4 0H196"));
        assert_eq!(flap, "M200 142H182V160Z");
    }
}
