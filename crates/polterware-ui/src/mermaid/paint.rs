//! Painting a [`Scene`] with GPUI 0.2.2.
//!
//! The scene is SVG's DOM without CSS, and this pass maps it onto the paint API: `PathBuilder`
//! for paths (filled and stroked, with `dash_array` for `stroke-dasharray`; GPUI has no line
//! caps, so they are its default butt caps), `paint_quad` for an axis-aligned rectangle's face
//! with corner radii, paths for ellipses and rotated boxes, `shape_line` and `ShapedLine::paint`
//! for text through the window's text system, and `paint_image` (the path `img` uses) for a
//! named image the caller resolves.
//!
//! A group's transform is composed into the path's own transform and handed to `PathBuilder`,
//! so curves and arcs stay in their scene coordinates until GPUI tessellates them. A stroke
//! keeps its scene width times the transform's scale times the zoom, because GPUI tessellates a
//! path after its transform. A group's clip becomes a content mask: GPUI clips to rectangles,
//! which is exactly what a [`Group::clip`](polterware_mermaid::scene::Group::clip) is.
//!
//! Text is the one thing GPUI cannot rotate or skew, so a rotated run is placed along its
//! rotation but its glyphs stay upright; the swimlane's vertical title is the only upstream run
//! that rotates. Images carry no opacity in the paint API either, so an image is painted at its
//! resolved opacity only by the resolver's bytes.
//!
//! Defaults come from the theme: a label with no fill is `color_text_primary`, a path's
//! `currentColor` is the theme's text color (or the placing path's stroke inside a marker), and
//! the view behind the scene is the caller's background. A scene's own CSS colors always win.

use std::sync::Arc;

use gpui::{
    App, BorderStyle, Bounds, ContentMask, Corners, FillOptions, Path as GpuiPath, PathBuilder,
    PathStyle as GpuiPathStyle, Pixels, Point as GpuiPoint, RenderImage, SharedString,
    StrokeOptions, TextRun as GpuiTextRun, UnderlineStyle, Window, fill, outline, point, px,
};

use polterware_mermaid::measure::TextMeasure;
use polterware_mermaid::scene::{
    EllipseShape, FillRule, ImageRef, Item, MarkerDef, MarkerOrient, MarkerRef, MarkerUnits,
    PathShape, PathStyle, Point, Rect, RectShape, Scene, TextAnchor, TextBaseline, TextRun,
    Transform,
};

use crate::mermaid::color::{css_color, with_alpha};
use crate::mermaid::geometry::{PathGeometry, scale_of, transformed_bounds};
use crate::mermaid::hit::SceneHit;
use crate::mermaid::measure::SceneMeasure;
use crate::mermaid::viewport::SceneViewport;
use crate::theme::{Theme, fade};

/// Resolves a scene image name to pixels. A scene carries only the name (`fa:fa-bell`, a URL,
/// an app asset); the app owns the bytes. Returning `None` leaves the image unpainted.
pub type ImageResolver = Arc<dyn Fn(&str) -> Option<Arc<RenderImage>>>;

/// Paints `scene` in `bounds`, at `viewport`, with the theme's fonts and default colors.
pub fn paint_scene(
    scene: &Scene,
    bounds: Bounds<Pixels>,
    viewport: SceneViewport,
    theme: Theme,
    images: Option<&ImageResolver>,
    window: &mut Window,
    cx: &mut App,
) {
    let measure = SceneMeasure::new(
        window.text_system().clone(),
        theme.font_sans,
        theme.font_mono,
    );
    let painter = ScenePainter {
        scene,
        theme,
        viewport,
        bounds,
        images,
        measure,
    };
    let mask = ContentMask { bounds };
    window.with_content_mask(Some(mask), |window| painter.paint(window, cx));
}

/// Outlines the element a hit test found, in the theme's primary color.
pub fn paint_selection(
    hit: &SceneHit,
    bounds: Bounds<Pixels>,
    viewport: SceneViewport,
    theme: Theme,
    window: &mut Window,
) {
    let rect = viewport.window_rect(hit.bounds, bounds);
    if f32::from(rect.size.width) <= 0.0 || f32::from(rect.size.height) <= 0.0 {
        return;
    }
    let padded = Bounds::new(
        point(rect.origin.x - px(2.0), rect.origin.y - px(2.0)),
        gpui::size(rect.size.width + px(4.0), rect.size.height + px(4.0)),
    );
    window.paint_quad(
        outline(padded, fade(theme.color_primary, 0.8), BorderStyle::Solid).corner_radii(px(3.0)),
    );
}

struct ScenePainter<'a> {
    scene: &'a Scene,
    theme: Theme,
    viewport: SceneViewport,
    bounds: Bounds<Pixels>,
    images: Option<&'a ImageResolver>,
    measure: SceneMeasure,
}

impl ScenePainter<'_> {
    fn paint(&self, window: &mut Window, cx: &mut App) {
        if let Some(background) = &self.scene.background
            && let Some(color) = css_color(&background.0, self.theme.color_text_primary)
        {
            window.paint_quad(fill(self.bounds, color));
        }
        self.paint_items(&self.scene.items, Transform::IDENTITY, 1.0, window, cx);
    }

    fn paint_items(
        &self,
        items: &[Item],
        transform: Transform,
        opacity: f64,
        window: &mut Window,
        cx: &mut App,
    ) {
        for item in items {
            match item {
                Item::Group(group) => {
                    let group_opacity = opacity * group.opacity;
                    if group_opacity <= 0.0 {
                        continue;
                    }
                    let world = group.transform.then(transform);
                    if let Some(clip) = group.clip {
                        let mask = self.mask_for(clip, world);
                        let inner = group_opacity;
                        let items = &group.items;
                        window.with_content_mask(Some(mask), |window| {
                            self.paint_items(items, world, inner, window, cx);
                        });
                    } else {
                        self.paint_items(&group.items, world, group_opacity, window, cx);
                    }
                }
                Item::Path(shape) => self.paint_path_shape(
                    shape,
                    transform,
                    opacity,
                    self.theme.color_text_primary,
                    window,
                    cx,
                ),
                Item::Rect(shape) => self.paint_rect_shape(shape, transform, opacity, window),
                Item::Ellipse(shape) => self.paint_ellipse(shape, transform, opacity, window),
                Item::Text(text) => self.paint_text(text, transform, opacity, window, cx),
                Item::Image(image) => self.paint_image(image, transform, window, cx),
            }
        }
    }

    fn mask_for(&self, clip: Rect, transform: Transform) -> ContentMask<Pixels> {
        ContentMask {
            bounds: self
                .viewport
                .window_rect(transformed_bounds(clip, transform), self.bounds),
        }
    }

    fn paint_path_shape(
        &self,
        shape: &PathShape,
        transform: Transform,
        opacity: f64,
        current: gpui::Rgba,
        window: &mut Window,
        cx: &mut App,
    ) {
        let style = &shape.style;
        let resolved_stroke = style
            .stroke
            .as_ref()
            .and_then(|stroke| css_color(&stroke.0, current));
        let inherited = resolved_stroke.unwrap_or(current);

        self.paint_path_fill(
            &shape.path,
            style,
            shape.fill_rule,
            transform,
            opacity,
            inherited,
            window,
        );
        self.paint_path_stroke(&shape.path, style, transform, opacity, inherited, window);

        if shape.start_markers.is_empty() && shape.end_markers.is_empty() {
            return;
        }
        let geometry = PathGeometry::from_path(&shape.path);
        let marker_color = resolved_stroke.unwrap_or(current);
        let stroke_width = style.stroke_width;
        self.paint_markers(
            &geometry,
            &shape.start_markers,
            true,
            transform,
            stroke_width,
            marker_color,
            opacity,
            window,
            cx,
        );
        self.paint_markers(
            &geometry,
            &shape.end_markers,
            false,
            transform,
            stroke_width,
            marker_color,
            opacity,
            window,
            cx,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_markers(
        &self,
        geometry: &PathGeometry,
        markers: &[MarkerRef],
        start: bool,
        transform: Transform,
        stroke_width: f64,
        current: gpui::Rgba,
        opacity: f64,
        window: &mut Window,
        cx: &mut App,
    ) {
        if markers.is_empty() {
            return;
        }
        let at = if start {
            geometry.first_point()
        } else {
            geometry.last_point()
        };
        let Some(at) = at else {
            return;
        };
        let tangent = if start {
            geometry.start_tangent()
        } else {
            geometry.end_tangent()
        };
        let angle = tangent
            .map(|tangent| tangent.y.atan2(tangent.x).to_degrees())
            .unwrap_or(0.0);
        for marker in markers {
            let Some(definition) = self.scene.marker_def(&marker.def) else {
                continue;
            };
            let color = marker
                .color
                .as_ref()
                .and_then(|color| css_color(&color.0, current))
                .unwrap_or(current);
            self.paint_marker(
                definition,
                at,
                angle,
                start,
                transform,
                stroke_width,
                color,
                opacity,
                window,
                cx,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_marker(
        &self,
        definition: &MarkerDef,
        at: Point,
        angle: f64,
        start: bool,
        transform: Transform,
        stroke_width: f64,
        current: gpui::Rgba,
        opacity: f64,
        window: &mut Window,
        cx: &mut App,
    ) {
        let view_box = definition.view_box;
        if view_box.width <= 0.0 || view_box.height <= 0.0 {
            return;
        }
        let units = match definition.units {
            MarkerUnits::StrokeWidth => stroke_width.max(0.0),
            MarkerUnits::UserSpaceOnUse => 1.0,
        };
        if units <= 0.0 {
            return;
        }
        let scale_x = units * definition.width / view_box.width;
        let scale_y = units * definition.height / view_box.height;
        let angle = match definition.orient {
            MarkerOrient::Auto => angle,
            MarkerOrient::AutoStartReverse => {
                if start {
                    angle + 180.0
                } else {
                    angle
                }
            }
            MarkerOrient::Fixed { degrees } => degrees,
        };
        let world = Transform::translate(-view_box.x, -view_box.y)
            .then(Transform::scale(scale_x, scale_y))
            .then(Transform::translate(
                -(definition.reference.x - view_box.x) * scale_x,
                -(definition.reference.y - view_box.y) * scale_y,
            ))
            .then(Transform::rotate(angle))
            .then(Transform::translate(at.x, at.y))
            .then(transform);
        for shape in &definition.shapes {
            self.paint_path_shape(shape, world, opacity, current, window, cx);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_path_fill(
        &self,
        path: &polterware_mermaid::scene::Path,
        style: &PathStyle,
        fill_rule: FillRule,
        transform: Transform,
        opacity: f64,
        current: gpui::Rgba,
        window: &mut Window,
    ) {
        let Some(fill) = &style.fill else {
            return;
        };
        let Some(color) = css_color(&fill.0, current) else {
            return;
        };
        let alpha = opacity * style.opacity * style.fill_opacity;
        if alpha <= 0.0 {
            return;
        }
        if let Some(built) = self.build_path(path, Some(fill_rule), None, transform, None) {
            window.paint_path(built, with_alpha(color, alpha));
        }
    }

    fn paint_path_stroke(
        &self,
        path: &polterware_mermaid::scene::Path,
        style: &PathStyle,
        transform: Transform,
        opacity: f64,
        current: gpui::Rgba,
        window: &mut Window,
    ) {
        let Some(stroke) = &style.stroke else {
            return;
        };
        let Some(color) = css_color(&stroke.0, current) else {
            return;
        };
        let alpha = opacity * style.opacity * style.stroke_opacity;
        let width = style.stroke_width * self.path_scale(transform);
        if alpha <= 0.0 || width <= 0.0 {
            return;
        }
        let dash: Vec<Pixels> = style
            .dash
            .iter()
            .filter(|value| **value > 0.0)
            .map(|value| px((value * self.path_scale(transform)) as f32))
            .collect();
        if let Some(built) = self.build_path(
            path,
            None,
            Some(width),
            transform,
            (!dash.is_empty()).then_some(dash.as_slice()),
        ) {
            window.paint_path(built, with_alpha(color, alpha));
        }
    }

    /// Scene units to screen pixels for a path: the group's scale and the viewport's zoom.
    fn path_scale(&self, transform: Transform) -> f64 {
        scale_of(transform) * self.viewport.zoom as f64
    }

    fn screen_transform(&self, transform: Transform) -> gpui::Transform {
        let origin = self.bounds.origin;
        let screen = Transform {
            a: self.viewport.zoom as f64,
            b: 0.0,
            c: 0.0,
            d: self.viewport.zoom as f64,
            e: self.viewport.x as f64 + f32::from(origin.x) as f64,
            f: self.viewport.y as f64 + f32::from(origin.y) as f64,
        };
        to_gpui_transform(transform.then(screen))
    }

    fn build_path(
        &self,
        path: &polterware_mermaid::scene::Path,
        fill_rule: Option<FillRule>,
        stroke_width: Option<f64>,
        transform: Transform,
        dash: Option<&[Pixels]>,
    ) -> Option<GpuiPath<Pixels>> {
        if path.is_empty() {
            return None;
        }
        let mut builder = match (fill_rule, stroke_width) {
            (Some(rule), _) => PathBuilder::fill().with_style(GpuiPathStyle::Fill(match rule {
                FillRule::EvenOdd => FillOptions::even_odd(),
                FillRule::NonZero => FillOptions::non_zero(),
            })),
            (None, Some(width)) => PathBuilder::stroke(px(width as f32)).with_style(
                GpuiPathStyle::Stroke(StrokeOptions::default().with_line_width(width as f32)),
            ),
            (None, None) => return None,
        };
        if let Some(dash) = dash {
            builder = builder.dash_array(dash);
        }
        push_commands(&mut builder, path);
        builder.transform(self.screen_transform(transform));
        builder.build().ok()
    }

    fn paint_rect_shape(
        &self,
        shape: &RectShape,
        transform: Transform,
        opacity: f64,
        window: &mut Window,
    ) {
        let current = self.theme.color_text_primary;
        let fill_color = shape
            .style
            .fill
            .as_ref()
            .and_then(|fill| css_color(&fill.0, current));
        let alpha = opacity * shape.style.opacity * shape.style.fill_opacity;
        let path = polterware_mermaid::scene::Path::rect(shape.rect, shape.rx, shape.ry);
        let axis_aligned = transform.b.abs() < 1e-9 && transform.c.abs() < 1e-9;
        match (fill_color, alpha > 0.0, axis_aligned) {
            (Some(color), true, true) => {
                let rect = self
                    .viewport
                    .window_rect(transformed_bounds(shape.rect, transform), self.bounds);
                let radius = shape
                    .rx
                    .min(shape.rect.width / 2.0)
                    .min(shape.rect.height / 2.0)
                    * self.path_scale(transform);
                window.paint_quad(
                    fill(rect, with_alpha(color, alpha)).corner_radii(px(radius as f32)),
                );
            }
            (Some(_), true, false) => self.paint_path_fill(
                &path,
                &shape.style,
                FillRule::NonZero,
                transform,
                opacity,
                current,
                window,
            ),
            _ => {}
        }
        self.paint_path_stroke(&path, &shape.style, transform, opacity, current, window);
    }

    fn paint_ellipse(
        &self,
        shape: &EllipseShape,
        transform: Transform,
        opacity: f64,
        window: &mut Window,
    ) {
        if shape.radius_x <= 0.0 || shape.radius_y <= 0.0 {
            return;
        }
        let path = ellipse_path(shape.center, shape.radius_x, shape.radius_y);
        let current = self.theme.color_text_primary;
        self.paint_path_fill(
            &path,
            &shape.style,
            FillRule::NonZero,
            transform,
            opacity,
            current,
            window,
        );
        self.paint_path_stroke(&path, &shape.style, transform, opacity, current, window);
    }

    fn paint_text(
        &self,
        text: &TextRun,
        transform: Transform,
        opacity: f64,
        window: &mut Window,
        cx: &mut App,
    ) {
        if text.lines.is_empty() || text.font.size <= 0.0 {
            return;
        }
        let color = text
            .fill
            .as_ref()
            .and_then(|fill| css_color(&fill.0, self.theme.color_text_primary))
            .unwrap_or(self.theme.color_text_primary);
        let alpha = opacity * text.opacity;
        if alpha <= 0.0 {
            return;
        }
        let color = with_alpha(color, alpha);
        let scale = self.path_scale(transform);
        if scale <= 0.0 {
            return;
        }
        let font_size = text.font.size * scale;
        if font_size < 0.5 {
            return;
        }
        let line_height = if text.line_height > 0.0 {
            text.line_height * scale
        } else {
            font_size * 1.2
        };
        let block_top = match text.baseline {
            TextBaseline::Alphabetic | TextBaseline::Bottom => text.position.y - text.size.height,
            TextBaseline::Middle | TextBaseline::Central => {
                text.position.y - text.size.height / 2.0
            }
            TextBaseline::Hanging | TextBaseline::Top => text.position.y,
        };
        for (index, line) in text.lines.iter().enumerate() {
            let local = Point::new(text.position.x, block_top + index as f64 * text.line_height);
            let local = if text.rotation == 0.0 {
                local
            } else {
                let offset = Point::new(local.x - text.position.x, local.y - text.position.y)
                    .rotated(text.rotation);
                text.position.translated(offset.x, offset.y)
            };
            let top = self.viewport.to_window(transform.apply(local), self.bounds);
            let mut content = String::new();
            let mut runs = Vec::with_capacity(line.spans.len());
            for span in &line.spans {
                if span.text.is_empty() {
                    continue;
                }
                let font = self.measure.span_font(&text.font, span);
                content.push_str(&span.text);
                runs.push(GpuiTextRun {
                    len: span.text.len(),
                    font: self.measure.gpui_font(&font),
                    color: color.into(),
                    background_color: None,
                    underline: span.link.as_ref().map(|_| UnderlineStyle {
                        thickness: px(1.0),
                        color: Some(color.into()),
                        wavy: false,
                    }),
                    strikethrough: None,
                });
            }
            if content.is_empty() {
                continue;
            }
            let shaped =
                self.measure
                    .shape_line(SharedString::from(content), px(font_size as f32), &runs);
            let width = f32::from(shaped.width);
            let x = f32::from(top.x)
                - match text.anchor {
                    TextAnchor::Start => 0.0,
                    TextAnchor::Middle => width / 2.0,
                    TextAnchor::End => width,
                };
            let _ = shaped.paint(point(px(x), top.y), px(line_height as f32), window, cx);
        }
    }

    fn paint_image(&self, image: &ImageRef, transform: Transform, window: &mut Window, cx: &App) {
        if let Some(tint) = &image.tint {
            let rect = self
                .viewport
                .window_rect(transformed_bounds(image.rect, transform), self.bounds);
            let Some(color) = css_color(&tint.0, self.theme.color_text_primary) else {
                return;
            };
            let _ = window.paint_svg(
                rect,
                image.name.clone().into(),
                gpui::TransformationMatrix::unit(),
                with_alpha(color, image.opacity).into(),
                cx,
            );
            return;
        }
        let Some(resolver) = self.images else {
            return;
        };
        let Some(pixels) = resolver(&image.name) else {
            return;
        };
        let rect = self
            .viewport
            .window_rect(transformed_bounds(image.rect, transform), self.bounds);
        if f32::from(rect.size.width) <= 0.0 || f32::from(rect.size.height) <= 0.0 {
            return;
        }
        let _ = window.paint_image(rect, Corners::all(px(0.0)), pixels, 0, false);
    }
}

fn to_gpui_transform(transform: Transform) -> gpui::Transform {
    gpui::Transform::new(
        transform.a as f32,
        transform.b as f32,
        transform.c as f32,
        transform.d as f32,
        transform.e as f32,
        transform.f as f32,
    )
}

fn push_commands(builder: &mut PathBuilder, path: &polterware_mermaid::scene::Path) {
    use polterware_mermaid::scene::PathCommand;
    for command in &path.commands {
        match *command {
            PathCommand::MoveTo { x, y } => builder.move_to(scene_point(x, y)),
            PathCommand::LineTo { x, y } => builder.line_to(scene_point(x, y)),
            PathCommand::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => builder.cubic_bezier_to(
                scene_point(x, y),
                scene_point(c1x, c1y),
                scene_point(c2x, c2y),
            ),
            PathCommand::QuadTo { cx, cy, x, y } => {
                builder.curve_to(scene_point(x, y), scene_point(cx, cy));
            }
            PathCommand::ArcTo {
                rx,
                ry,
                rotation,
                large_arc,
                sweep,
                x,
                y,
            } => builder.arc_to(
                point(px(rx as f32), px(ry as f32)),
                px(rotation as f32),
                large_arc,
                sweep,
                scene_point(x, y),
            ),
            PathCommand::Close => builder.close(),
        }
    }
}

fn scene_point(x: f64, y: f64) -> GpuiPoint<Pixels> {
    point(px(x as f32), px(y as f32))
}

fn ellipse_path(center: Point, radius_x: f64, radius_y: f64) -> polterware_mermaid::scene::Path {
    let mut path = polterware_mermaid::scene::Path::new();
    path.move_to(Point::new(center.x + radius_x, center.y));
    path.arc_to(
        radius_x,
        radius_y,
        0.0,
        false,
        true,
        Point::new(center.x - radius_x, center.y),
    );
    path.arc_to(
        radius_x,
        radius_y,
        0.0,
        false,
        true,
        Point::new(center.x + radius_x, center.y),
    );
    path.close();
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ellipse_path_closes_around_its_center() {
        let path = ellipse_path(Point::new(10.0, 10.0), 5.0, 3.0);
        assert_eq!(path.commands.len(), 4);
        assert!(matches!(
            path.commands.last(),
            Some(polterware_mermaid::scene::PathCommand::Close)
        ));
        let geometry = PathGeometry::from_path(&path);
        let bounds = geometry.bounds().expect("bounds");
        assert!((bounds.x - 5.0).abs() < 1e-6);
        assert!((bounds.width - 10.0).abs() < 1e-6);
    }

    #[test]
    fn scene_transforms_keep_svg_column_order() {
        let transform = Transform::translate(10.0, 20.0).then(Transform::scale(2.0, 2.0));
        let gpui = to_gpui_transform(transform);
        assert_eq!(gpui.m31, 20.0);
        assert_eq!(gpui.m32, 40.0);
    }
}
