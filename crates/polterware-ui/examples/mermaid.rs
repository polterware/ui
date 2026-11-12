//! A small hand-written Mermaid scene in a [`MermaidView`]: two nodes, a labelled edge with an
//! arrowhead, a rounded box, an ellipse, and a rotated group, to show the painter's paths,
//! quads, text, markers, and transforms without needing the layout pipeline.
//!
//! `cargo run --example mermaid -p polterware-ui` opens it. Drag empty space or scroll to pan,
//! command/control-scroll to zoom around the pointer, and double-click to fit.

use gpui::{
    App, AppContext as _, Application, Bounds, WindowBounds, WindowKind, WindowOptions, point, px,
    size,
};

use polterware_mermaid::Font;
use polterware_mermaid::scene::{
    Color, EllipseShape, Group, MarkerDef, MarkerOrient, MarkerRef, Path, PathShape, PathStyle,
    Point, Rect, RectShape, Scene, SourceRef, TextBlock, TextLine, TextRun, TextSpan, Transform,
};
use polterware_ui::IconAssets;
use polterware_ui::mermaid::MermaidView;
use polterware_ui::theme;

fn main() {
    Application::new()
        .with_assets(IconAssets)
        .run(|cx: &mut App| {
            theme::set_root_rem(13.0);
            polterware_ui::init(cx);
            let scene = demo_scene();
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(880.0), px(560.0)),
                    cx,
                ))),
                kind: WindowKind::Normal,
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("Mermaid scene".into()),
                    appears_transparent: false,
                    traffic_light_position: Some(point(px(12.0), px(12.0))),
                }),
                ..Default::default()
            };
            cx.open_window(options, move |_, cx| cx.new(|_| MermaidView::new(scene)))
                .expect("the example window");
            cx.activate(true);
        });
}

fn demo_scene() -> Scene {
    let mut scene = Scene::fitted(Rect::new(0.0, 0.0, 620.0, 360.0), 20.0);

    let mut arrow = MarkerDef::new("arrow", Rect::new(0.0, 0.0, 10.0, 10.0))
        .with_reference(Point::new(9.0, 5.0))
        .with_size(9.0, 9.0)
        .with_orient(MarkerOrient::Auto)
        .with_class("marker flowchart-v2");
    let mut head = Path::new();
    head.move_to(Point::new(0.0, 0.0))
        .line_to(Point::new(10.0, 5.0))
        .line_to(Point::new(0.0, 10.0))
        .close();
    arrow.push(PathShape::new(head, PathStyle::filled(Color::current())));
    scene.define(arrow);

    // Start: a rounded box in the flowchart's lavender.
    let start = Rect::new(60.0, 50.0, 160.0, 64.0);
    scene.push(
        RectShape::new(
            start,
            10.0,
            PathStyle::filled("#ECECFF").with_stroke("#9370DB", 1.5),
        )
        .with_source(SourceRef::node("Start")),
    );
    scene.push(
        TextRun::centered(
            start.center(),
            Font::new("Arial", 14.0),
            text_block(vec![TextSpan::plain("Collect input")], 14.0),
        )
        .with_fill("#333333")
        .with_source(SourceRef::node("Start")),
    );

    // Middle: an ellipse, the classic decision shape's cousin.
    let middle = Point::new(350.0, 90.0);
    scene.push(
        EllipseShape::new(
            middle,
            80.0,
            40.0,
            PathStyle::filled("#FFF3BF").with_stroke("#B8860B", 1.5),
        )
        .with_source(SourceRef::node("Decide")),
    );
    scene.push(
        TextRun::centered(
            middle,
            Font::new("Arial", 13.0),
            text_block(vec![TextSpan::bold("Decide")], 13.0),
        )
        .with_fill("#333333")
        .with_source(SourceRef::node("Decide")),
    );

    // The edge leaves the box's right side, curves under, and points into the ellipse.
    let mut edge = Path::new();
    edge.move_to(Point::new(start.right(), start.center().y))
        .cubic_to(
            Point::new(260.0, start.center().y),
            Point::new(250.0, middle.y),
            Point::new(middle.x - 80.0, middle.y),
        );
    scene.push(
        PathShape::new(
            edge,
            PathStyle::stroked("#555555", 2.0).with_dash([6.0, 4.0]),
        )
        .with_source(SourceRef::edge("Start-Decide"))
        .with_end_marker(MarkerRef::new("arrow").with_color("#555555")),
    );
    scene.push(
        TextRun::centered(
            Point::new(255.0, 55.0),
            Font::new("Arial", 12.0),
            text_block(vec![TextSpan::italic("yes")], 12.0),
        )
        .with_fill("#555555")
        .with_source(SourceRef::edge("Start-Decide")),
    );

    // A cluster frame with a rotated title, drawn in its own translated group.
    let mut cluster = Group::translated(60.0, 210.0)
        .with_source(SourceRef::cluster("Notes"))
        .with_clip(Rect::new(0.0, 0.0, 500.0, 110.0));
    cluster.push(RectShape::new(
        Rect::new(0.0, 0.0, 500.0, 110.0),
        12.0,
        PathStyle::filled("#F8F8FF").with_stroke("#DDDDDD", 1.0),
    ));
    let mut title = TextRun::new(
        Point::new(18.0, 26.0),
        Font::new("Arial", 13.0),
        text_block(vec![TextSpan::bold("Notes")], 13.0),
    )
    .with_fill("#666666");
    title.source = Some(SourceRef::cluster("Notes"));
    cluster.push(title);
    cluster.push(
        TextRun::new(
            Point::new(18.0, 54.0),
            Font::new("Arial", 12.0),
            text_block(
                vec![
                    TextSpan::plain("The scene carries "),
                    TextSpan::bold("groups, paths, text"),
                    TextSpan::plain(", and markers."),
                ],
                12.0,
            ),
        )
        .with_fill("#444444")
        .with_source(SourceRef::cluster("Notes")),
    );

    // A rotated badge proves the group transform reaches the painter.
    let mut badge = Group::new()
        .with_transform(Transform::translate(330.0, 235.0).then(Transform::rotate(-8.0)))
        .with_source(SourceRef::node("Badge"));
    badge.push(RectShape::new(
        Rect::new(0.0, 0.0, 120.0, 34.0),
        17.0,
        PathStyle::filled("#E7F5FF").with_stroke("#74C0FC", 1.0),
    ));
    badge.push(
        TextRun::centered(
            Point::new(60.0, 17.0),
            Font::new("Arial", 12.0),
            text_block(vec![TextSpan::plain("rotated group")], 12.0),
        )
        .with_fill("#1864AB"),
    );
    cluster.push(badge);
    scene.push(cluster);

    scene
}

fn text_block(spans: Vec<TextSpan>, size: f64) -> TextBlock {
    let width = spans.iter().map(|span| span.text.len()).sum::<usize>() as f64 * size * 0.52;
    TextBlock::new(
        vec![TextLine::new(spans)],
        polterware_mermaid::scene::Size::new(width, size * 1.15),
        size * 1.1,
    )
}
