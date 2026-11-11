//! Cluster shapes from upstream `rendering-util/rendering-elements/clusters.js`.
//!
//! CSS shape counterparts are Mermaid's `.cluster`, `.outer`, `.inner`, and `.divider` rules;
//! the grouping and title-strip behavior follows Mermaid's cluster renderers. Unlike their D3
//! wrappers, these functions return scene groups directly. TODO: port rough.js `handDrawn`
//! output, palette slots, CSS-derived styles, title text spans, and exact `intersectRect` updates.

use crate::scene::{
    Color, Group, Item, Path, PathStyle, Point, Rect, RectShape, Scene, Size, SourceRef,
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

/// Render registered upstream cluster shapes: `rect`, `roundedWithTitle`, `noteGroup`,
/// `divider`, `kanbanSection`, `agentCluster`, `flowCluster`, and `usecaseSystemBoundary`.
pub fn render_cluster(
    node: &ClusterNode,
    scene: &mut Scene,
    cluster_background: &str,
    cluster_border: &str,
) -> ClusterGeometry {
    let titled = !matches!(node.shape.as_str(), "noteGroup" | "divider");
    let needed = node.label_width + node.padding;
    let width = node.width.max(needed);
    let diff = if node.width <= needed {
        (width - node.width) / 2.0 - node.padding
    } else {
        -node.padding
    };
    let source = SourceRef::cluster(node.id.clone());
    let mut g = group(
        node,
        match node.shape.as_str() {
            "noteGroup" => "note-cluster",
            "roundedWithTitle" => "",
            "divider" => "",
            _ => "cluster",
        },
    );
    let outer_h = if node.shape == "roundedWithTitle" {
        node.height
    } else {
        node.height
    };
    let mut background = cluster_background;
    let mut border = cluster_border;
    if node.shape == "divider" {
        background = "none";
        border = cluster_border;
    }
    if node.shape == "noteGroup" {
        background = "none";
        border = "none";
    }
    g.push(shape_rect(
        node,
        if node.shape == "noteGroup" {
            node.width
        } else {
            width
        },
        outer_h,
        node.rx.max(node.ry),
        Some(Color::new(background)),
        Some(Color::new(border)),
    ));
    let mut offset_y = if titled && node.shape != "roundedWithTitle" {
        node.label_height - node.padding / 2.0
    } else {
        0.0
    };
    if node.shape == "roundedWithTitle" {
        let inner_y = node.y - node.height / 2.0 + node.label_height + 2.0;
        let inner_h = (node.height - node.label_height - 6.0).max(0.0);
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
        g.push(inner);
        offset_y = node.label_height - node.padding / 2.0;
    }
    if titled && node.label_height > 0.0 {
        let mut label = Group::new()
            .with_class("cluster-label")
            .with_source(source.clone());
        label.transform = crate::scene::Transform::translate(
            node.x - node.label_width / 2.0,
            node.y - node.height / 2.0,
        );
        g.push(label);
    }
    if node.shape == "divider" {
        let mut p = Path::new();
        p.move_to(Point::new(node.x - width / 2.0, node.y))
            .line_to(Point::new(node.x + width / 2.0, node.y));
        let mut style = PathStyle::default();
        style.fill = None;
        style.stroke = Some(Color::new(cluster_border));
        style.dash = vec![5.0];
        g.push(crate::scene::PathShape::new(p, style).with_source(source));
    }
    scene.push(Item::Group(g.clone()));
    ClusterGeometry {
        group: g,
        label_bounds: Size::new(
            node.label_width,
            if titled { node.label_height } else { 0.0 },
        ),
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
