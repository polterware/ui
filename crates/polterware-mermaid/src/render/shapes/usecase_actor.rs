//! Classic port of upstream `shapes/usecaseActor.ts`, including the shared renderer the
//! awesome, hollow and icon variants use.
//!
//! Look: the stick-figure use case actor of the web design system: a 56 × 72 figure with the
//! label below it and an invisible outline rect the layout sizes from.
//! Behavior: upstream `usecaseActor.ts`'s `renderUsecaseActor`, with its padding, stereotype
//! stack, business marker, label placement and rect intersection. [`draw_with`] takes the
//! business flag, stereotype and icon the upstream node carries, because this crate's shared
//! `Node` has none of those fields yet; the variant files call it with their glyph.
//!
//! The per-item colour slot, the ARIA role and the leaf classes (`usecase-actor-stick`,
//! `usecase-actor-hollow-head`, ...) belong to the DOM; the scene keeps the shape and glyph
//! group classes. The icon glyph is an [`crate::scene::ImageRef`] the painter resolves; upstream's
//! unknown-icon fallback picture is not reproduced. Upstream's `padding ?? 8` only covers a
//! node without a padding, which a [`Node`] cannot express, so its padding is used as given.
//! `handDrawn` is not rendered: rough.js jitter stays a renderer TODO.

use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, get_node_classes, prepare,
    shape_style,
};
use crate::measure::Font;
use crate::render::labels::{LabelOptions, create_label};
use crate::scene::{
    Group, ImageRef, Item, Path, PathShape, PathStyle, Point, Rect, RectShape, Size, SourceRef,
    Transform,
};

// handDrawn: rough.js (later)

const ACTOR_FIGURE_WIDTH: f64 = 56.0;
const ACTOR_FIGURE_HEIGHT: f64 = 72.0;
const ACTOR_LABEL_GAP: f64 = 8.0;
const STEREOTYPE_LABEL_GAP: f64 = 2.0;
const ICON_FRAME_SIZE: f64 = 52.0;
const ICON_SIZE: f64 = 42.0;
const ICON_CENTER_Y: f64 = -2.0;

/// The four upstream actor variants; each draws a different glyph inside the shared frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorVariant {
    Normal,
    Hollow,
    Awesome,
    Icon,
}

impl ActorVariant {
    fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Hollow => "hollow",
            Self::Awesome => "awesome",
            Self::Icon => "icon",
        }
    }

    /// `variantClass`.
    fn glyph_class(self) -> &'static str {
        match self {
            Self::Normal => "usecase-actor-shape usecase-actor-normal",
            Self::Hollow => "usecase-actor-hollow",
            Self::Awesome => "usecase-actor-awesome",
            Self::Icon => "usecase-actor-icon",
        }
    }
}

/// The optional node data upstream reads from `UsecaseActorNode`.
#[derive(Clone, Debug, Default)]
pub struct ActorOptions<'a> {
    /// Renders the `usecase-business-marker` line for a business actor.
    pub business: bool,
    /// Renders `«stereotype»` in its own label above the actor label.
    pub stereotype: Option<&'a str>,
    /// The icon name an icon actor draws inside its frame, as the painter resolves it.
    pub icon: Option<&'a str>,
}

// TODO(dedupe): sibling usecase shapes measure extra labels the same way; this mirrors
// `insertLabel` for a centered SVG label.
fn measure_label(
    text: &str,
    class: &str,
    node_id: &str,
    ctx: &ShapeContext<'_>,
) -> Result<(Group, Size), ShapeError> {
    let label = create_label(
        &LabelOptions {
            text: text.to_string(),
            font: Font::default(),
            markdown: false,
            width: f64::INFINITY,
            centered: true,
            class: class.to_string(),
            source: Some(SourceRef::node(node_id)),
        },
        ctx.measure,
    )
    .map_err(|error| ShapeError {
        message: error.to_string(),
    })?;
    let size = match label.items.first() {
        Some(Item::Text(text)) => text.size,
        _ => Size::ZERO,
    };
    Ok((label, size))
}

/// `positionLabel` for an SVG label: the bbox origin is the text origin, so it centres the
/// measured box on `(0, center_y)`.
fn position_label(label: &mut Group, size: Size, center_y: f64) {
    label.transform = Transform::translate(-size.width / 2.0, center_y - size.height / 2.0);
}

fn stereotype_text(stereotype: &str) -> String {
    format!("«{stereotype}»")
}

/// `STICK_FIGURE_PATH`: head circle, torso, arms and legs as one path.
fn stick_figure_path() -> Path {
    let mut path = Path::new();
    path.move_to(Point::new(0.0, -12.0))
        .cubic_to(
            Point::new(6.627, -12.0),
            Point::new(12.0, -17.373),
            Point::new(12.0, -24.0),
        )
        .cubic_to(
            Point::new(12.0, -30.627),
            Point::new(6.627, -36.0),
            Point::new(0.0, -36.0),
        )
        .cubic_to(
            Point::new(-6.627, -36.0),
            Point::new(-12.0, -30.627),
            Point::new(-12.0, -24.0),
        )
        .cubic_to(
            Point::new(-12.0, -17.373),
            Point::new(-6.627, -12.0),
            Point::new(0.0, -12.0),
        )
        .close()
        .move_to(Point::new(0.0, -12.0))
        .line_to(Point::new(0.0, 8.0))
        .move_to(Point::new(-17.0, -5.0))
        .line_to(Point::new(17.0, -5.0))
        .move_to(Point::new(0.0, 8.0))
        .line_to(Point::new(-15.0, 28.0))
        .move_to(Point::new(0.0, 8.0))
        .line_to(Point::new(15.0, 28.0));
    path
}

/// `AWESOME_SILHOUETTE_PATH`: a head and shoulders silhouette.
fn awesome_silhouette_path() -> Path {
    let mut path = Path::new();
    path.move_to(Point::new(0.0, -34.0))
        .cubic_to(
            Point::new(7.18, -34.0),
            Point::new(13.0, -28.18),
            Point::new(13.0, -21.0),
        )
        .cubic_to(
            Point::new(13.0, -13.82),
            Point::new(7.18, -8.0),
            Point::new(0.0, -8.0),
        )
        .cubic_to(
            Point::new(-7.18, -8.0),
            Point::new(-13.0, -13.82),
            Point::new(-13.0, -21.0),
        )
        .cubic_to(
            Point::new(-13.0, -28.18),
            Point::new(-7.18, -34.0),
            Point::new(0.0, -34.0),
        )
        .close()
        .move_to(Point::new(-24.0, 25.0))
        .cubic_to(
            Point::new(-24.0, 7.0),
            Point::new(-14.0, -3.0),
            Point::new(0.0, -3.0),
        )
        .cubic_to(
            Point::new(14.0, -3.0),
            Point::new(24.0, 7.0),
            Point::new(24.0, 25.0),
        )
        .cubic_to(
            Point::new(24.0, 28.0),
            Point::new(21.0, 30.0),
            Point::new(18.0, 30.0),
        )
        .line_to(Point::new(-18.0, 30.0))
        .cubic_to(
            Point::new(-21.0, 30.0),
            Point::new(-24.0, 28.0),
            Point::new(-24.0, 25.0),
        )
        .close();
    path
}

/// `HOLLOW_BODY_PATH`: the blocky body of the hollow variant.
fn hollow_body_path() -> Path {
    let mut path = Path::new();
    path.move_to(Point::new(-22.0, -10.0))
        .line_to(Point::new(22.0, -10.0))
        .line_to(Point::new(22.0, 0.0))
        .line_to(Point::new(6.0, 0.0))
        .line_to(Point::new(22.0, 17.0))
        .line_to(Point::new(13.0, 28.0))
        .line_to(Point::new(0.0, 13.0))
        .line_to(Point::new(-13.0, 28.0))
        .line_to(Point::new(-22.0, 17.0))
        .line_to(Point::new(-6.0, 0.0))
        .line_to(Point::new(-22.0, 0.0))
        .close();
    path
}

const BUSINESS_MARKER_ANGLE: f64 = std::f64::consts::PI / 3.0;
const BUSINESS_MARKER_OFFSET_RATIO: f64 = 0.6;

/// `businessMarkerPathForCircle`, the diagonal across the actor's head.
fn business_marker_path_for_circle(center_y: f64, radius: f64) -> Path {
    let center_offset = radius * BUSINESS_MARKER_OFFSET_RATIO;
    let half_chord = radius * (1.0 - BUSINESS_MARKER_OFFSET_RATIO.powi(2)).sqrt();
    let direction_x = BUSINESS_MARKER_ANGLE.cos();
    let direction_y = -BUSINESS_MARKER_ANGLE.sin();
    let center_x = center_offset * -direction_y;
    let marker_center_y = center_y + center_offset * direction_x;
    let delta_x = half_chord * direction_x;
    let delta_y = half_chord * direction_y;

    let mut path = Path::new();
    path.move_to(Point::new(center_x - delta_x, marker_center_y - delta_y))
        .line_to(Point::new(center_x + delta_x, marker_center_y + delta_y));
    path
}

/// `businessMarkerPath`.
fn business_marker_path(variant: ActorVariant) -> Path {
    match variant {
        ActorVariant::Normal => business_marker_path_for_circle(-24.0, 12.0),
        ActorVariant::Hollow => business_marker_path_for_circle(-23.0, 9.0),
        ActorVariant::Awesome => business_marker_path_for_circle(-21.0, 13.0),
        ActorVariant::Icon => {
            let mut path = Path::new();
            path.move_to(Point::new(12.0, -8.0))
                .line_to(Point::new(26.0, -26.0));
            path
        }
    }
}

fn outline_style() -> PathStyle {
    PathStyle {
        opacity: 0.0,
        ..shape_style()
    }
}

fn hollow_style() -> PathStyle {
    PathStyle {
        fill: None,
        ..shape_style()
    }
}

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    draw_with(node, ctx, ActorVariant::Normal, &ActorOptions::default())
}

/// `renderUsecaseActor`: the shared frame, label stack and glyph of every actor variant.
pub fn draw_with(
    node: &Node,
    ctx: &ShapeContext<'_>,
    variant: ActorVariant,
    options: &ActorOptions<'_>,
) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, mut label, label_box) = prepare(&node, ctx)?;
    group.class = Some(get_node_classes(
        &node,
        Some(&format!(
            "usecase-actor-variant usecase-actor-{}",
            variant.name()
        )),
    ));
    label.class = Some("label actor-label usecase-actor-label".to_string());

    let mut stereotype_label = None;
    if let Some(stereotype) = options.stereotype {
        let (label_group, label_size) = measure_label(
            &stereotype_text(stereotype),
            "label usecase-stereotype",
            &node.id,
            ctx,
        )?;
        stereotype_label = Some((label_group, label_size));
    }

    let padding = node.padding;
    let stereotype_height = stereotype_label
        .as_ref()
        .map(|(_, size)| size.height)
        .unwrap_or(0.0);
    let content_height = ACTOR_FIGURE_HEIGHT
        + ACTOR_LABEL_GAP
        + label_box.height
        + if stereotype_label.is_some() {
            stereotype_height + STEREOTYPE_LABEL_GAP
        } else {
            0.0
        };
    let total_width = ACTOR_FIGURE_WIDTH.max(label_box.width).max(
        stereotype_label
            .as_ref()
            .map(|(_, size)| size.width)
            .unwrap_or(0.0),
    ) + padding * 2.0;
    let total_height = content_height + padding * 2.0;
    let content_top = -content_height / 2.0;

    let glyph_transform = Transform::translate(0.0, content_top + ACTOR_FIGURE_HEIGHT / 2.0);

    let mut next_label_center = content_top + ACTOR_FIGURE_HEIGHT + ACTOR_LABEL_GAP;
    if let Some((label_group, size)) = stereotype_label.as_mut() {
        next_label_center += size.height / 2.0;
        position_label(label_group, *size, next_label_center);
        next_label_center += size.height / 2.0 + STEREOTYPE_LABEL_GAP;
    }
    position_label(
        &mut label,
        label_box,
        next_label_center + label_box.height / 2.0,
    );

    let mut glyph = Group::new()
        .with_class(format!("usecase-actor-glyph {}", variant.glyph_class()))
        .with_transform(glyph_transform);
    glyph.source = Some(SourceRef::node(node.id.clone()));
    match variant {
        ActorVariant::Normal => {
            glyph.push(PathShape::new(stick_figure_path(), shape_style()));
        }
        ActorVariant::Awesome => {
            glyph.push(PathShape::new(awesome_silhouette_path(), shape_style()));
        }
        ActorVariant::Hollow => {
            glyph.push(crate::scene::EllipseShape::new(
                Point::new(0.0, -23.0),
                9.0,
                9.0,
                hollow_style(),
            ));
            glyph.push(PathShape::new(hollow_body_path(), hollow_style()));
        }
        ActorVariant::Icon => {
            let frame = Rect::new(
                -ICON_FRAME_SIZE / 2.0,
                ICON_CENTER_Y - ICON_FRAME_SIZE / 2.0,
                ICON_FRAME_SIZE,
                ICON_FRAME_SIZE,
            );
            glyph.push(RectShape::new(frame, 4.0, shape_style()));
            if let Some(icon) = options.icon {
                // Upstream centres the icon svg (`translate(-w/2 - x, centerY - h/2 - y)`);
                // the painter's asset supplies the drawn shape.
                glyph.push(ImageRef::new(
                    icon,
                    Rect::new(
                        -ICON_SIZE / 2.0,
                        ICON_CENTER_Y - ICON_SIZE / 2.0,
                        ICON_SIZE,
                        ICON_SIZE,
                    ),
                ));
            }
        }
    }
    if options.business {
        glyph.push(PathShape::new(
            business_marker_path(variant),
            hollow_style(),
        ));
    }

    // `insert('rect', ':first-child')`: the invisible outline paints first and sizes the node.
    let outline = Rect::new(
        -total_width / 2.0,
        -total_height / 2.0,
        total_width,
        total_height,
    );
    let mut outline_shape = RectShape::new(outline, 0.0, outline_style());
    outline_shape.source = Some(SourceRef::node(node.id.clone()));
    group.items.insert(0, Item::Rect(outline_shape));
    group.items.push(Item::Group(label));
    if let Some((label_group, _)) = stereotype_label {
        group.items.push(Item::Group(label_group));
    }
    group.items.push(Item::Group(glyph));

    node.bounds = Rect::new(
        -total_width / 2.0,
        -total_height / 2.0,
        total_width,
        total_height,
    );
    node.intersect = IntersectKind::Rect;
    Ok(RenderedNode { node, group })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;
    use crate::theme::ThemeVariables;

    #[test]
    fn figure_height_and_padding_size_the_outline() {
        let mut node = Node::new("a", "Actor", "usecaseActor");
        node.padding = 10.0;
        let rendered = draw(
            &node,
            &ShapeContext {
                measure: &FallbackMeasure,
                theme: &ThemeVariables::default(),
            },
        )
        .unwrap();
        let label_height = match &rendered.group.items[1] {
            Item::Group(label) => match label.items.first() {
                Some(Item::Text(text)) => text.size.height,
                _ => panic!("the label is measured"),
            },
            _ => panic!("the label follows the outline"),
        };
        assert!((rendered.node.bounds.height - (72.0 + 8.0 + label_height + 20.0)).abs() < 1e-9);
        assert!(rendered.group.items.len() == 3);
    }

    #[test]
    fn business_marker_only_for_business_actors() {
        let node = Node::new("a", "Actor", "usecaseActor");
        let plain = draw(
            &node,
            &ShapeContext {
                measure: &FallbackMeasure,
                theme: &ThemeVariables::default(),
            },
        )
        .unwrap();
        let business = draw_with(
            &node,
            &ShapeContext {
                measure: &FallbackMeasure,
                theme: &ThemeVariables::default(),
            },
            ActorVariant::Normal,
            &ActorOptions {
                business: true,
                ..ActorOptions::default()
            },
        )
        .unwrap();
        let glyph_paths = |rendered: &RenderedNode| match rendered.group.items.last() {
            Some(Item::Group(glyph)) => glyph
                .items
                .iter()
                .filter(|item| matches!(item, Item::Path(_)))
                .count(),
            _ => 0,
        };
        assert_eq!(glyph_paths(&plain), 1);
        assert_eq!(glyph_paths(&business), 2);
    }

    #[test]
    fn icon_frame_is_fifty_two_points_wide() {
        let node = Node::new("a", "Actor", "usecaseActorIcon");
        let rendered = draw_with(
            &node,
            &ShapeContext {
                measure: &FallbackMeasure,
                theme: &ThemeVariables::default(),
            },
            ActorVariant::Icon,
            &ActorOptions {
                icon: Some("fa:fa-user"),
                ..ActorOptions::default()
            },
        )
        .unwrap();
        let glyph = match rendered.group.items.last() {
            Some(Item::Group(glyph)) => glyph,
            _ => panic!("glyph group"),
        };
        match &glyph.items[0] {
            Item::Rect(frame) => assert_eq!(frame.rect, Rect::new(-26.0, -28.0, 52.0, 52.0)),
            _ => panic!("frame rect"),
        }
        assert!(matches!(glyph.items[1], Item::Image(_)));
    }

    #[test]
    fn matches_upstream_fixtures() {
        use crate::render::shapes::usecase_ellipse::difftest;
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../testdata/shapes/usecaseActor.json"))
                .expect("fixture parses");
        let theme = ThemeVariables::default();
        for case in fixture["cases"].as_array().expect("cases") {
            let measure = difftest::FixedMeasure::from_case(case);
            let node = difftest::node_for(case, "usecaseActor");
            let rendered = draw_with(
                &node,
                &ShapeContext {
                    measure: &measure,
                    theme: &theme,
                },
                ActorVariant::Normal,
                &ActorOptions {
                    business: case["business"].as_bool().unwrap_or(false),
                    stereotype: case["stereotype"].as_str(),
                    icon: None,
                },
            )
            .expect("shape renders");
            difftest::assert_case(&rendered, case);
        }
    }
}
