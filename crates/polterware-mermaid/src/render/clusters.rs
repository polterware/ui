//! Cluster shapes from upstream `rendering-util/rendering-elements/clusters.js`.
//!
//! CSS shape counterparts are Mermaid's `.cluster`, `.outer`, `.inner`, and `.divider` rules;
//! the grouping and title-strip behavior follows Mermaid's cluster renderers. Unlike their D3
//! wrappers, these functions return scene groups directly. Rough.js output is represented by
//! its classic geometry; painters can use the retained `data-look` role to select a later pass.

use crate::measure::{FallbackMeasure, Font, TextMeasure, TextOptions, shape_text};
use crate::scene::{
    Color, Group, Item, Path, PathStyle, Point, Rect, RectShape, Scene, Size, SourceRef, TextBlock,
    TextLine, TextRun, TextSpan,
};

/// Cluster geometry and label bounds. `width` and `height` include cluster padding adjustments.
#[derive(Clone, Debug, PartialEq)]
pub struct ClusterNode {
    pub id: String,
    pub shape: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub padding: f64,
    pub rx: f64,
    pub ry: f64,
    pub label_width: f64,
    pub label_height: f64,
    pub classes: String,
    pub label: String,
    pub color_index: Option<usize>,
    pub look: String,
    pub direction: String,
    pub boundary_type: String,
    pub content_top: Option<f64>,
    pub font: Font,
}

impl ClusterNode {
    pub fn new(id: impl Into<String>, x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            id: id.into(),
            shape: "rect".into(),
            x,
            y,
            width,
            height,
            padding: 0.0,
            rx: 0.0,
            ry: 0.0,
            label_width: 0.0,
            label_height: 0.0,
            classes: String::new(),
            label: String::new(),
            color_index: None,
            look: "classic".into(),
            direction: "TB".into(),
            boundary_type: "rect".into(),
            content_top: None,
            font: Font::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClusterGeometry {
    pub group: Group,
    pub label_bounds: Size,
    pub diff: f64,
    pub offset_x: f64,
    pub offset_y: f64,
}

/// SVG rectangle intersection used by `intersect-rect.js`.
pub fn intersect_rect(center: Point, size: Size, toward: Point) -> Point {
    let dx = toward.x - center.x;
    let dy = toward.y - center.y;
    if dx == 0.0 && dy == 0.0 {
        return center;
    }
    let sx = if dx == 0.0 {
        f64::INFINITY
    } else {
        (size.width / 2.0) / dx.abs()
    };
    let sy = if dy == 0.0 {
        f64::INFINITY
    } else {
        (size.height / 2.0) / dy.abs()
    };
    let t = sx.min(sy);
    Point::new(center.x + dx * t, center.y + dy * t)
}

fn resolve_title(node: &ClusterNode, measure: &dyn TextMeasure) -> TextBlock {
    if node.label.is_empty() {
        TextBlock::default()
    } else {
        shape_text(&node.label, &TextOptions::unwrapped(), &node.font, measure).unwrap_or_default()
    }
}

/// Mermaid's twelve-slot palette selection. The slot is a stable style role, not a color copy.
pub fn color_slot(index: Option<usize>) -> Option<String> {
    index.map(|i| format!("data-color-id-color-{}", i % 12))
}

pub fn cluster_paints_title(shape: Option<&str>) -> bool {
    !matches!(shape.unwrap_or("rect"), "noteGroup" | "divider")
}

pub fn cluster_shape_registered(shape: &str) -> bool {
    matches!(
        shape,
        "rect"
            | "squareRect"
            | "roundedWithTitle"
            | "noteGroup"
            | "divider"
            | "kanbanSection"
            | "flowGroup"
            | "usecaseSystemBoundary"
            | "swimlane"
            | "pool"
    )
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoundaryGeometry {
    pub width: f64,
    pub height: f64,
    pub x: f64,
    pub y: f64,
    pub body_y: f64,
    pub body_height: f64,
    pub tab_height: f64,
    pub tab_width: f64,
}
pub fn usecase_system_boundary_geometry(node: &ClusterNode, label: Size) -> BoundaryGeometry {
    let package = node.boundary_type == "package";
    let tab_height = if package { label.height + 10.0 } else { 0.0 };
    let width = node
        .width
        .max(label.width + if package { 20.0 } else { node.padding });
    let height = if package {
        node.height.max(tab_height + node.padding * 2.0)
    } else {
        node.height
    };
    let x = node.x - width / 2.0;
    let y = node.y - height / 2.0;
    BoundaryGeometry {
        width,
        height,
        x,
        y,
        body_y: y + tab_height,
        body_height: height - tab_height,
        tab_height,
        tab_width: if package {
            width.min(80.0_f64.max(label.width + 20.0))
        } else {
            0.0
        },
    }
}

fn shape_rect(
    node: &ClusterNode,
    width: f64,
    height: f64,
    radius: f64,
    fill: Option<Color>,
    stroke: Option<Color>,
) -> RectShape {
    let mut style = PathStyle::default();
    style.fill = fill;
    style.stroke = stroke;
    style.stroke_width = 1.0;
    RectShape::new(
        Rect::new(node.x - width / 2.0, node.y - height / 2.0, width, height),
        radius,
        style,
    )
    .with_source(SourceRef::cluster(node.id.clone()))
}

fn group(node: &ClusterNode, class: &str) -> Group {
    Group::new()
        .with_class(if node.classes.is_empty() {
            class.to_string()
        } else {
            format!("{class} {}", node.classes)
        })
        .with_source(SourceRef::cluster(node.id.clone()))
}
fn style_role(role: &str, item: impl Into<Item>, source: SourceRef) -> Group {
    let mut group = Group::new().with_class(role).with_source(source);
    group.push(item);
    group
}

/// Render registered upstream cluster shapes: `rect`, `roundedWithTitle`, `noteGroup`,
/// `divider`, `kanbanSection`, `flowGroup`, `usecaseSystemBoundary`, `swimlane`, and `pool`.
pub fn render_cluster(
    node: &ClusterNode,
    scene: &mut Scene,
    cluster_background: &str,
    cluster_border: &str,
) -> ClusterGeometry {
    render_cluster_measured(
        node,
        scene,
        cluster_background,
        cluster_border,
        &FallbackMeasure,
    )
}

/// Render a cluster with the caller's text system, preserving the title as scene text.
pub fn render_cluster_measured(
    node: &ClusterNode,
    scene: &mut Scene,
    cluster_background: &str,
    cluster_border: &str,
    measure: &dyn TextMeasure,
) -> ClusterGeometry {
    let measured = resolve_title(node, measure);
    let label_width = if node.label_width > 0.0 {
        node.label_width
    } else {
        measured.size.width
    };
    let label_height = if node.label_height > 0.0 {
        node.label_height
    } else {
        measured.size.height
    };
    let titled = cluster_paints_title(Some(&node.shape));
    let needed = label_width + node.padding;
    let width = node.width.max(needed);
    let diff = if node.width <= needed {
        (width - node.width) / 2.0 - node.padding
    } else {
        -node.padding
    };
    let source = SourceRef::cluster(node.id.clone());
    let role = format!(
        "{} data-look-{}{}",
        match node.shape.as_str() {
            "swimlane" => "cluster swimlane",
            "pool" => "cluster pool swimlane",
            "roundedWithTitle" => "composite-cluster",
            "usecaseSystemBoundary" => "cluster usecase-system-boundary",
            _ => "cluster",
        },
        node.look,
        color_slot(node.color_index)
            .map(|c| format!(" {c}"))
            .unwrap_or_default()
    );
    let mut g = group(node, &role);
    g.id = Some(node.id.clone());
    let outer_h = node.height;
    let mut background = cluster_background;
    let mut border = cluster_border;
    if node.shape == "noteGroup" {
        background = "none";
        border = "none";
    }
    if node.shape == "flowGroup" {
        background = "none";
    }
    if node.shape == "usecaseSystemBoundary" {
        let b = usecase_system_boundary_geometry(node, Size::new(label_width, label_height));
        g.push(
            RectShape::new(
                Rect::new(b.x, b.body_y, b.width, b.body_height),
                node.rx,
                PathStyle {
                    fill: Some(Color::new(cluster_background)),
                    stroke: Some(Color::new(cluster_border)),
                    ..PathStyle::default()
                },
            )
            .with_source(source.clone()),
        );
        if b.tab_height > 0.0 {
            g.push(
                RectShape::new(
                    Rect::new(b.x, b.y, b.tab_width, b.tab_height),
                    node.rx,
                    PathStyle {
                        fill: Some(Color::new(cluster_background)),
                        stroke: Some(Color::new(cluster_border)),
                        ..PathStyle::default()
                    },
                )
                .with_source(source.clone()),
            );
        }
    } else if node.shape == "swimlane" || node.shape == "pool" {
        let left = node.x - width / 2.0;
        let top = node.y - node.height / 2.0;
        if node.direction == "LR" {
            let title = label_height + 8.0;
            g.push(
                RectShape::new(
                    Rect::new(left, top, title, node.height),
                    node.rx,
                    PathStyle {
                        fill: Some(Color::new(cluster_background)),
                        stroke: Some(Color::new(cluster_border)),
                        ..PathStyle::default()
                    },
                )
                .with_source(source.clone()),
            );
            g.push(
                RectShape::new(
                    Rect::new(left + title, top, (width - title).max(0.0), node.height),
                    0.0,
                    PathStyle {
                        fill: Some(Color::new("none")),
                        stroke: Some(Color::new(cluster_border)),
                        ..PathStyle::default()
                    },
                )
                .with_source(source.clone()),
            );
        } else {
            let title =
                label_height.min((node.content_top.unwrap_or(top + label_height) - top).max(0.0));
            g.push(
                RectShape::new(
                    Rect::new(left, top, width, title),
                    node.rx,
                    PathStyle {
                        fill: Some(Color::new(cluster_background)),
                        stroke: Some(Color::new(cluster_border)),
                        ..PathStyle::default()
                    },
                )
                .with_source(source.clone()),
            );
            g.push(
                RectShape::new(
                    Rect::new(left, top + title, width, (node.height - title).max(0.0)),
                    0.0,
                    PathStyle {
                        fill: Some(Color::new("none")),
                        stroke: Some(Color::new(cluster_border)),
                        ..PathStyle::default()
                    },
                )
                .with_source(source.clone()),
            );
        }
    } else {
        g.push(style_role(
            "outer",
            shape_rect(
                node,
                if node.shape == "noteGroup" {
                    node.width
                } else {
                    width
                },
                outer_h,
                if node.shape == "flowGroup" {
                    10.0
                } else {
                    node.rx.max(node.ry)
                },
                Some(Color::new(background)),
                Some(Color::new(border)),
            ),
            source.clone(),
        ));
    }
    let mut offset_y = if titled && node.shape != "roundedWithTitle" {
        label_height - node.padding / 2.0
    } else {
        0.0
    };
    if node.shape == "roundedWithTitle" {
        let inner_y = node.y - node.height / 2.0 + label_height + 2.0;
        let inner_h = (node.height - label_height - 6.0).max(0.0);
        let inner = RectShape::new(
            Rect::new(node.x - width / 2.0, inner_y, width, inner_h),
            0.0,
            PathStyle {
                fill: Some(Color::new("var(--composite-background)")),
                stroke: Some(Color::new(cluster_border)),
                ..PathStyle::default()
            },
        )
        .with_source(source.clone());
        g.push(style_role("inner", inner, source.clone()));
        offset_y = label_height - node.padding / 2.0;
    }
    if titled && label_height > 0.0 && !node.label.trim().is_empty() {
        let mut label = Group::new()
            .with_class("cluster-label")
            .with_source(source.clone());
        if matches!(node.shape.as_str(), "swimlane" | "pool") && node.direction == "LR" {
            label.transform = crate::scene::Transform::translate(
                node.x - node.width / 2.0 + (label_height + 8.0) / 2.0,
                node.y,
            )
            .then(crate::scene::Transform::rotate(-90.0))
            .then(crate::scene::Transform::translate(
                -label_width / 2.0,
                -label_height / 2.0,
            ));
        } else if node.shape == "usecaseSystemBoundary" && node.boundary_type == "package" {
            let b = usecase_system_boundary_geometry(node, Size::new(label_width, label_height));
            label.transform = crate::scene::Transform::translate(
                b.x + (b.tab_width - label_width) / 2.0,
                b.y + (b.tab_height - label_height) / 2.0,
            );
        } else {
            label.transform = crate::scene::Transform::translate(
                node.x - label_width / 2.0,
                node.y - node.height / 2.0,
            );
        }
        let block = if measured.lines.is_empty() {
            TextBlock::new(
                vec![TextLine::new(vec![TextSpan::plain(node.label.clone())])],
                Size::new(label_width, label_height),
                measure.line_height(&node.font),
            )
        } else {
            measured
        };
        label.push(
            TextRun::new(Point::ZERO, node.font.clone(), block)
                .with_source(source.clone())
                .with_fill("currentColor"),
        );
        g.push(label);
    }
    if let Some(slot) = color_slot(node.color_index) {
        g.class = Some(format!("{} {slot}", g.class.unwrap_or_default()));
    }
    scene.push(Item::Group(g.clone()));
    ClusterGeometry {
        group: g,
        label_bounds: Size::new(label_width, if titled { label_height } else { 0.0 }),
        diff,
        offset_x: 0.0,
        offset_y,
    }
}

/// Produce the rounded rectangle SVG path used by cluster containers.
pub fn rounded_rect_path(x: f64, y: f64, width: f64, height: f64, radius: f64) -> Path {
    let r = radius
        .max(0.0)
        .min(width.abs() / 2.0)
        .min(height.abs() / 2.0);
    let mut p = Path::new();
    p.move_to(Point::new(x + r, y))
        .line_to(Point::new(x + width - r, y))
        .quad_to(Point::new(x + width, y), Point::new(x + width, y + r))
        .line_to(Point::new(x + width, y + height - r))
        .quad_to(
            Point::new(x + width, y + height),
            Point::new(x + width - r, y + height),
        )
        .line_to(Point::new(x + r, y + height))
        .quad_to(Point::new(x, y + height), Point::new(x, y + height - r))
        .line_to(Point::new(x, y + r))
        .quad_to(Point::new(x, y), Point::new(x + r, y))
        .close();
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn note_group_has_no_title_and_preserves_box() {
        let n = ClusterNode::new("n", 20.0, 30.0, 40.0, 50.0);
        let mut s = Scene::default();
        let g = render_cluster(
            &ClusterNode {
                shape: "noteGroup".into(),
                ..n
            },
            &mut s,
            "white",
            "black",
        );
        assert_eq!(g.label_bounds, Size::ZERO);
        assert_eq!(s.items.len(), 1);
    }
    #[test]
    fn titled_cluster_expands_for_label_and_offsets_children() {
        let n = ClusterNode {
            label_width: 80.0,
            label_height: 12.0,
            padding: 8.0,
            ..ClusterNode::new("c", 0.0, 0.0, 40.0, 60.0)
        };
        let mut s = Scene::default();
        let g = render_cluster(&n, &mut s, "white", "black");
        assert_eq!(g.diff, 16.0);
        assert_eq!(g.offset_y, 8.0);
    }
    #[test]
    fn rounded_path_clamps_radius_and_closes() {
        let p = rounded_rect_path(0.0, 0.0, 10.0, 8.0, 99.0);
        assert_eq!(p.commands.len(), 10);
        assert!(matches!(
            p.commands.last(),
            Some(crate::scene::PathCommand::Close)
        ));
    }
}
