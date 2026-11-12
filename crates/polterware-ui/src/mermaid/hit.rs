//! What the pointer is over: the topmost node, edge, or cluster that drew a pixel.
//!
//! A scene item carries a [`SourceRef`] when it belongs to the diagram's own element, and the
//! tree is painted in order, so the last item whose shape contains the point is the one on top.
//! That is the item a click should select. Groups pass their source down, which lets a cluster
//! be picked by its box or a node by its label.
//!
//! The walk takes each group's SVG transform with it and inverts it to test a shape in its own
//! coordinates, so rotation and scale work without special cases. Everything local is in scene
//! units, and `tolerance` arrives in scene units too (`screen_pixels / zoom`), so a thin edge
//! is as easy to hit on screen at every zoom. A group's clip rectangle is tested the same way:
//! what a clip hides cannot be clicked.

use polterware_mermaid::scene::{
    EllipseShape, Group, Item, PathShape, Point, Rect, Scene, SourceRef, Transform,
};

use crate::mermaid::geometry::{PathGeometry, inverse, scale_of, transformed_bounds};

/// The diagram element under the pointer, and where it was drawn in scene coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneHit {
    pub source: SourceRef,
    pub bounds: Rect,
}

/// The topmost item with a source under `point` (scene coordinates).
pub fn hit_test(scene: &Scene, point: Point, tolerance: f64) -> Option<SceneHit> {
    let mut best = None;
    walk(
        &scene.items,
        Transform::IDENTITY,
        None,
        point,
        tolerance,
        &mut best,
    );
    best
}

fn walk(
    items: &[Item],
    transform: Transform,
    inherited: Option<SourceRef>,
    point: Point,
    tolerance: f64,
    best: &mut Option<SceneHit>,
) {
    for item in items {
        let source = item.source().cloned().or_else(|| inherited.clone());
        match item {
            Item::Group(group) => {
                if group.opacity <= 0.0 {
                    continue;
                }
                let world = group.transform.then(transform);
                if group_clip_hides(group, world, point) {
                    continue;
                }
                walk(&group.items, world, source, point, tolerance, best);
            }
            _ => {
                let Some(local) = inverse(transform).map(|inverse| inverse.apply(point)) else {
                    continue;
                };
                let local_tolerance = tolerance / scale_of(transform).max(1e-9);
                if hits(item, local, local_tolerance)
                    && let Some(source) = source
                {
                    *best = Some(SceneHit {
                        source,
                        bounds: bounds_of(item, transform),
                    });
                }
            }
        }
    }
}

fn group_clip_hides(group: &Group, world: Transform, point: Point) -> bool {
    let Some(clip) = group.clip else {
        return false;
    };
    let Some(local) = inverse(world).map(|inverse| inverse.apply(point)) else {
        return true;
    };
    !clip.contains(local)
}

fn hits(item: &Item, point: Point, tolerance: f64) -> bool {
    match item {
        Item::Group(_) => false,
        Item::Path(path) => path_hit(path, point, tolerance),
        Item::Rect(rect) => expanded(rect.rect, tolerance).contains(point),
        Item::Ellipse(ellipse) => ellipse_hit(ellipse, point, tolerance),
        Item::Text(text) => expanded(text.bounds(), tolerance).contains(point),
        Item::Image(image) => expanded(image.rect, tolerance).contains(point),
    }
}

fn path_hit(path: &PathShape, point: Point, tolerance: f64) -> bool {
    let filled = path
        .style
        .fill
        .as_ref()
        .is_some_and(|fill| !is_none(fill.0.as_str()));
    let stroked = path
        .style
        .stroke
        .as_ref()
        .is_some_and(|stroke| !is_none(stroke.0.as_str()));
    if !filled && !stroked {
        return false;
    }
    let geometry = PathGeometry::from_path(&path.path);
    geometry.hit(
        point,
        path.fill_rule,
        filled,
        if stroked {
            path.style.stroke_width
        } else {
            0.0
        },
        tolerance,
    )
}

fn ellipse_hit(ellipse: &EllipseShape, point: Point, tolerance: f64) -> bool {
    if ellipse.radius_x <= 0.0 || ellipse.radius_y <= 0.0 {
        return false;
    }
    let dx = (point.x - ellipse.center.x) / (ellipse.radius_x + tolerance);
    let dy = (point.y - ellipse.center.y) / (ellipse.radius_y + tolerance);
    dx * dx + dy * dy <= 1.0
}

fn bounds_of(item: &Item, transform: Transform) -> Rect {
    match item {
        Item::Path(path) => PathGeometry::from_path(&path.path)
            .bounds()
            .map(|bounds| transformed_bounds(bounds, transform))
            .unwrap_or(Rect::ZERO),
        Item::Rect(rect) => transformed_bounds(rect.rect, transform),
        Item::Ellipse(ellipse) => transformed_bounds(
            Rect::new(
                ellipse.center.x - ellipse.radius_x,
                ellipse.center.y - ellipse.radius_y,
                ellipse.radius_x * 2.0,
                ellipse.radius_y * 2.0,
            ),
            transform,
        ),
        Item::Text(text) => transformed_bounds(text.bounds(), transform),
        Item::Image(image) => transformed_bounds(image.rect, transform),
        Item::Group(_) => Rect::ZERO,
    }
}

fn expanded(rect: Rect, padding: f64) -> Rect {
    if padding <= 0.0 {
        rect
    } else {
        rect.expanded(padding)
    }
}

fn is_none(color: &str) -> bool {
    color.trim().eq_ignore_ascii_case("none")
}

#[cfg(test)]
mod tests {
    use super::*;
    use polterware_mermaid::scene::{
        EllipseShape, Group, MarkerRef, Path, PathStyle, Point, RectShape, SourceRef, TextBlock,
        TextRun, Transform,
    };

    fn node(id: &str, rect: Rect) -> Item {
        RectShape::new(rect, 4.0, PathStyle::filled("#ECECFF"))
            .with_source(SourceRef::node(id))
            .into()
    }

    #[test]
    fn a_node_is_picked_inside_its_box() {
        let mut scene = Scene::default();
        scene.push(node("A", Rect::new(10.0, 10.0, 80.0, 40.0)));
        let hit = hit_test(&scene, Point::new(50.0, 30.0), 2.0).expect("inside");
        assert_eq!(hit.source, SourceRef::node("A"));
        assert_eq!(hit.bounds, Rect::new(10.0, 10.0, 80.0, 40.0));
        assert!(hit_test(&scene, Point::new(150.0, 30.0), 2.0).is_none());
    }

    #[test]
    fn the_topmost_item_wins() {
        let mut scene = Scene::default();
        scene.push(node("A", Rect::new(0.0, 0.0, 100.0, 100.0)));
        scene.push(node("B", Rect::new(50.0, 50.0, 100.0, 100.0)));
        let hit = hit_test(&scene, Point::new(75.0, 75.0), 0.0).expect("inside");
        assert_eq!(hit.source, SourceRef::node("B"));
    }

    #[test]
    fn a_group_transform_moves_what_is_picked() {
        let mut group = Group::translated(100.0, 20.0).with_source(SourceRef::cluster("sub"));
        group.push(node("A", Rect::new(0.0, 0.0, 40.0, 20.0)));
        let mut scene = Scene::default();
        scene.push(group);
        assert!(hit_test(&scene, Point::new(50.0, 30.0), 0.0).is_none());
        let hit = hit_test(&scene, Point::new(120.0, 30.0), 0.0).expect("moved");
        assert_eq!(hit.source, SourceRef::node("A"));
    }

    #[test]
    fn a_clip_hides_what_it_cuts() {
        let mut group = Group::new().with_clip(Rect::new(0.0, 0.0, 20.0, 20.0));
        group.push(node("A", Rect::new(0.0, 0.0, 100.0, 100.0)));
        let mut scene = Scene::default();
        scene.push(group);
        assert!(hit_test(&scene, Point::new(10.0, 10.0), 0.0).is_some());
        assert!(hit_test(&scene, Point::new(60.0, 60.0), 0.0).is_none());
    }

    #[test]
    fn a_group_source_is_inherited_by_an_unlabelled_child() {
        let mut group = Group::new().with_source(SourceRef::cluster("sub"));
        group.push(RectShape::new(
            Rect::new(0.0, 0.0, 20.0, 20.0),
            0.0,
            PathStyle::filled("#fff"),
        ));
        let mut scene = Scene::default();
        scene.push(group);
        let hit = hit_test(&scene, Point::new(5.0, 5.0), 0.0).expect("inside");
        assert_eq!(hit.source, SourceRef::cluster("sub"));
    }

    #[test]
    fn an_edge_is_picked_near_its_path_and_not_its_box() {
        let mut path = Path::new();
        path.move_to(Point::new(0.0, 0.0))
            .line_to(Point::new(200.0, 0.0));
        let edge = PathShape::new(path, PathStyle::stroked("#333333", 2.0))
            .with_source(SourceRef::edge("A-B"));
        let mut scene = Scene::default();
        scene.push(edge);
        assert!(hit_test(&scene, Point::new(100.0, 0.5), 4.0).is_some());
        assert!(hit_test(&scene, Point::new(100.0, 20.0), 4.0).is_none());
        let _ = MarkerRef::new("arrow");
    }

    #[test]
    fn a_label_hit_returns_its_source() {
        let block = TextBlock::new(
            Vec::new(),
            polterware_mermaid::scene::Size::new(40.0, 12.0),
            13.0,
        );
        let text = TextRun::centered(Point::new(50.0, 20.0), Default::default(), block)
            .with_source(SourceRef::edge("A-B"));
        let mut scene = Scene::default();
        scene.push(text);
        let hit = hit_test(&scene, Point::new(50.0, 15.0), 0.0).expect("inside");
        assert_eq!(hit.source, SourceRef::edge("A-B"));
    }

    #[test]
    fn an_ellipse_is_picked_inside_its_curve() {
        let mut scene = Scene::default();
        scene.push(
            EllipseShape::new(
                Point::new(50.0, 50.0),
                20.0,
                10.0,
                PathStyle::filled("#fff"),
            )
            .with_source(SourceRef::node("E")),
        );
        assert!(hit_test(&scene, Point::new(50.0, 50.0), 0.0).is_some());
        assert!(hit_test(&scene, Point::new(50.0, 65.0), 0.0).is_none());
    }

    #[test]
    fn a_rotated_group_is_picked_through_its_inverse() {
        let mut group = Group::new().with_transform(Transform::rotate(90.0));
        group.push(node("A", Rect::new(10.0, 0.0, 20.0, 10.0)));
        let mut scene = Scene::default();
        scene.push(group);
        // (10, 0)..(30, 10) turned 90 degrees is x -10..0, y 10..30.
        assert!(hit_test(&scene, Point::new(-5.0, 20.0), 0.0).is_some());
        assert!(hit_test(&scene, Point::new(20.0, 5.0), 0.0).is_none());
    }
}
