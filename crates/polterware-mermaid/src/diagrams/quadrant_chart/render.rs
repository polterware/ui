//! Source-to-[`Scene`] rendering for quadrant charts: the Rust side of `quadrantRenderer.ts`
//! `draw` and of the geometry `quadrantBuilder.ts` computes (`getQuadrantData`, `build`).
//!
//! A quadrant chart does not go through the unified layout pipeline: `draw` uses plain d3. It
//! reads the `QuadrantBuilderData` from [`quadrantDb.ts`](super::db) (the title, the four
//! quadrant labels, the four axis labels, and the points with their classes), merges the chart
//! configuration and the theme variables, and appends five groups to the SVG: `quadrants`
//! (four rects with their labels), `border` (six lines), `data-points` (a circle and a label
//! per point), `labels` (the axis labels), and `title`. This module ports that: [`draw`]
//! parses, [`draw_ast`] renders an already parsed database, and [`render`] uses the crate's
//! monospace fallback measurement.
//!
//! Every coordinate is the builder's arithmetic, not a measurement: `calculateSpace` reserves
//! the title and the axis labels around a `chartWidth` × `chartHeight` square,
//! `getQuadrants` splits it in four, `getQuadrantPoints` maps `[0, 1]` through d3's
//! `scaleLinear`, and `getBorders` strokes the outer and inner edges. The title's and labels'
//! anchors are SVG's `dominant-baseline`/`text-anchor`, carried as [`TextBaseline`] and
//! [`TextAnchor`]. Text is measured in the `monospace` family, the rule the fixture harness
//! renders with and that [`FallbackMeasure`](crate::measure::FallbackMeasure) reproduces.
//!
//! Where the port stops short of upstream: the class names (`main`, `quadrant`, `data-point`,
//! ...) matter only to CSS, and a [`Scene`] carries paints rather than selectors, so `draw`
//! resolves the declarations into the shapes. The scene keeps the same group structure so a
//! painter can still tell a quadrant from a point. Every SVG paint value is the theme's string,
//! including the default `quadrantPointFill` (`hsl(240, 100%, NaN%)`), which the theme module
//! reproduces exactly.
//!
//! `showXAxis`, `showYAxis` and `showTitle` are `QuadrantBuilderConfig` fields that the JSON
//! config never sets, so they keep the builder defaults (all `true`); the renderer folds them
//! into "the diagram has such a label" here, as `build` does. An empty quadrant label is still
//! emitted as an empty text run, exactly as `draw` appends an empty `<text>` for it.

use crate::config::{Config, QuadrantChartConfig};
use crate::error::ParseError;
use crate::measure::{Font, TextMeasure, TextOptions, shape_text};
use crate::scene::{
    Color, EllipseShape, Group, Path, PathShape, PathStyle, Point as ScenePoint, Rect, RectShape,
    Scene, TextAnchor, TextBaseline, TextRun,
};

use super::Ast;

/// A text label's vertical anchor source, upstream's `TextVerticalPos`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextVerticalPos {
    Left,
    Center,
}

/// A text label's horizontal baseline source, upstream's `TextHorizontalPos`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextHorizontalPos {
    Top,
    Middle,
}

/// One label the builder produces, upstream's `QuadrantTextType`: the text, its paint, its
/// `translate` position, and its SVG baseline and anchor.
#[derive(Clone, Debug)]
struct TextItem {
    text: String,
    fill: Option<String>,
    x: f64,
    y: f64,
    font_size: f64,
    vertical_pos: TextVerticalPos,
    horizontal_pos: TextHorizontalPos,
    rotation: f64,
}

/// One point after `getQuadrantPoints`, upstream's `QuadrantPointType`.
#[derive(Clone, Debug)]
struct PointItem {
    x: f64,
    y: f64,
    fill: Option<String>,
    radius: f64,
    text: TextItem,
    stroke_color: Option<String>,
    stroke_width: f64,
}

/// One quadrant after `getQuadrants`, upstream's `QuadrantQuadrantsType`.
#[derive(Clone, Debug)]
struct QuadrantItem {
    text: TextItem,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    fill: Option<String>,
}

/// One border line after `getBorders`, upstream's `QuadrantLineType`.
#[derive(Clone, Debug)]
struct LineItem {
    stroke_width: f64,
    stroke_fill: Option<String>,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
}

/// Everything `build` hands the renderer, upstream's `QuadrantBuildType`.
#[derive(Clone, Debug, Default)]
struct Build {
    points: Vec<PointItem>,
    quadrants: Vec<QuadrantItem>,
    axis_labels: Vec<TextItem>,
    title: Option<TextItem>,
    border_lines: Vec<LineItem>,
}

/// The parts of `calculateSpace`'s result the getters read, upstream's `CalculateSpaceData`.
struct Space {
    title_space_top: f64,
    quadrant_left: f64,
    quadrant_top: f64,
    quadrant_width: f64,
    quadrant_half_width: f64,
    quadrant_height: f64,
    quadrant_half_height: f64,
}

/// Renders a quadrant chart body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(body: &str, config: &Config, measure: &dyn TextMeasure) -> Result<Scene, ParseError> {
    let ast = super::parse(body)?;
    Ok(draw_ast(&ast, config, measure))
}

/// Renders an already parsed quadrant chart database.
pub fn draw_ast(ast: &Ast, config: &Config, measure: &dyn TextMeasure) -> Scene {
    draw_ast_with_theme(ast, config, &config.theme_variables, measure)
}

/// Renders a quadrant chart body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, ParseError> {
    draw(body, config, &crate::measure::FallbackMeasure)
}

/// `getDominantBaseLine`: `top` is SVG's hanging baseline, everything else the middle.
fn dominant_baseline(horizontal_pos: TextHorizontalPos) -> TextBaseline {
    match horizontal_pos {
        TextHorizontalPos::Top => TextBaseline::Hanging,
        TextHorizontalPos::Middle => TextBaseline::Middle,
    }
}

/// `getTextAnchor`: `left` starts at the anchor point, everything else is centered.
fn text_anchor(vertical_pos: TextVerticalPos) -> TextAnchor {
    match vertical_pos {
        TextVerticalPos::Left => TextAnchor::Start,
        TextVerticalPos::Center => TextAnchor::Middle,
    }
}

/// A theme variable that is a string, as `getConfig().themeVariables` stores colors.
fn theme_string<'a>(
    theme: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    theme.get(key).and_then(serde_json::Value::as_str)
}

/// `scaleLinear().domain([0, 1]).range([from, to])` for one value.
fn scale_linear(value: f64, from: f64, to: f64) -> f64 {
    from + (to - from) * value
}

/// A point's `x` as `scaleLinear` coerces the token text: JavaScript's `Number(text)`.
fn point_coordinate(text: &str) -> f64 {
    text.parse::<f64>().unwrap_or(f64::NAN)
}

/// `parseFloat` of a `stroke-width` style (`"3px"`), which validation keeps to digits plus
/// `px`; an absent value is the builder's `"0px"`.
fn stroke_width(value: Option<&str>) -> f64 {
    value
        .and_then(|value| value.strip_suffix("px"))
        .and_then(|digits| digits.parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// Renders an already parsed database with the theme variables `getQuadrantData` merges.
fn draw_ast_with_theme(
    ast: &Ast,
    config: &Config,
    theme: &serde_json::Map<String, serde_json::Value>,
    measure: &dyn TextMeasure,
) -> Scene {
    let quadrant_config = &config.quadrant_chart;
    let build = build(ast, quadrant_config, theme);

    let width = quadrant_config.chart_width;
    let height = quadrant_config.chart_height;
    let view_box = Rect::new(0.0, 0.0, width, height);

    let mut main = Group::new().with_class("main");

    // `quadrantsGroup`: one `g.quadrant` per quadrant, each a rect and its label.
    let mut quadrants_group = Group::new().with_class("quadrants");
    for quadrant in &build.quadrants {
        let mut group = Group::new().with_class("quadrant");
        group.push(RectShape::new(
            Rect::new(quadrant.x, quadrant.y, quadrant.width, quadrant.height),
            0.0,
            PathStyle {
                fill: quadrant.fill.as_deref().map(Color::new),
                ..PathStyle::default()
            },
        ));
        group.push(text_run(&quadrant.text, measure));
        quadrants_group.push(group);
    }
    main.push(quadrants_group);

    // `borderGroup`: six straight lines, without round caps or joins.
    let mut border_group = Group::new().with_class("border");
    for line in &build.border_lines {
        let mut path = Path::new();
        path.move_to(ScenePoint::new(line.x1, line.y1));
        path.line_to(ScenePoint::new(line.x2, line.y2));
        border_group.push(PathShape::new(
            path,
            PathStyle {
                fill: None,
                stroke: line.stroke_fill.as_deref().map(Color::new),
                stroke_width: line.stroke_width,
                ..PathStyle::default()
            },
        ));
    }
    main.push(border_group);

    // `dataPointGroup`: each point is its label and a circle.
    let mut data_point_group = Group::new().with_class("data-points");
    for point in &build.points {
        let mut group = Group::new().with_class("data-point");
        group.push(EllipseShape::new(
            ScenePoint::new(point.x, point.y),
            point.radius,
            point.radius,
            PathStyle {
                fill: point.fill.as_deref().map(Color::new),
                stroke: point.stroke_color.as_deref().map(Color::new),
                stroke_width: point.stroke_width,
                ..PathStyle::default()
            },
        ));
        group.push(text_run(&point.text, measure));
        data_point_group.push(group);
    }
    main.push(data_point_group);

    // `labelGroup`: one `g.label` per axis label.
    let mut label_group = Group::new().with_class("labels");
    for label in &build.axis_labels {
        let mut group = Group::new().with_class("label");
        group.push(text_run(label, measure));
        label_group.push(group);
    }
    main.push(label_group);

    // `titleGroup`: at most one text element.
    if let Some(title) = &build.title {
        let mut title_group = Group::new().with_class("title");
        title_group.push(text_run(title, measure));
        main.push(title_group);
    }

    let mut scene = Scene::new(view_box);
    scene.push(main);
    scene
}

/// `QuadrantBuilder::build`: compute the labels, then the space they and the title leave the
/// quadrant square.
fn build(
    ast: &Ast,
    config: &QuadrantChartConfig,
    theme: &serde_json::Map<String, serde_json::Value>,
) -> Build {
    let show_x_axis = !(ast.x_axis_left_text.is_empty() && ast.x_axis_right_text.is_empty());
    let show_y_axis = !(ast.y_axis_bottom_text.is_empty() && ast.y_axis_top_text.is_empty());
    let show_title = !ast.diagram_title.is_empty();

    // The x axis moves below the quadrants as soon as there is a point to read against it.
    let x_axis_position = if ast.points.is_empty() {
        config.x_axis_position.as_str()
    } else {
        "bottom"
    };

    let space = calculate_space(
        config,
        x_axis_position,
        show_x_axis,
        show_y_axis,
        show_title,
    );

    Build {
        points: get_quadrant_points(ast, config, theme, &space),
        quadrants: get_quadrants(ast, config, theme, &space),
        axis_labels: get_axis_labels(
            ast,
            config,
            theme,
            x_axis_position,
            show_x_axis,
            show_y_axis,
            &space,
        ),
        border_lines: get_borders(config, theme, &space),
        title: get_title(ast, config, theme, show_title),
    }
}

/// `calculateSpace`: the title and axis paddings, then the quadrant square inside them.
fn calculate_space(
    config: &QuadrantChartConfig,
    x_axis_position: &str,
    show_x_axis: bool,
    show_y_axis: bool,
    show_title: bool,
) -> Space {
    let x_axis_space_calculation =
        config.x_axis_label_padding * 2.0 + config.x_axis_label_font_size;
    let x_axis_space_top = if x_axis_position == "top" && show_x_axis {
        x_axis_space_calculation
    } else {
        0.0
    };
    let x_axis_space_bottom = if x_axis_position == "bottom" && show_x_axis {
        x_axis_space_calculation
    } else {
        0.0
    };

    let y_axis_space_calculation =
        config.y_axis_label_padding * 2.0 + config.y_axis_label_font_size;
    let y_axis_space_left = if config.y_axis_position == "left" && show_y_axis {
        y_axis_space_calculation
    } else {
        0.0
    };
    let y_axis_space_right = if config.y_axis_position == "right" && show_y_axis {
        y_axis_space_calculation
    } else {
        0.0
    };

    let title_space_calculation = config.title_font_size + config.title_padding * 2.0;
    let title_space_top = if show_title {
        title_space_calculation
    } else {
        0.0
    };

    let quadrant_left = config.quadrant_padding + y_axis_space_left;
    let quadrant_top = config.quadrant_padding + x_axis_space_top + title_space_top;
    let quadrant_width =
        config.chart_width - config.quadrant_padding * 2.0 - y_axis_space_left - y_axis_space_right;
    let quadrant_height = config.chart_height
        - config.quadrant_padding * 2.0
        - x_axis_space_top
        - x_axis_space_bottom
        - title_space_top;

    Space {
        title_space_top,
        quadrant_left,
        quadrant_top,
        quadrant_width,
        quadrant_half_width: quadrant_width / 2.0,
        quadrant_height,
        quadrant_half_height: quadrant_height / 2.0,
    }
}

/// `getAxisLabels`: the x labels at the top or bottom, the y labels rotated `-90` at the left
/// or right, and each pair centered on its half when both sides are present.
fn get_axis_labels(
    ast: &Ast,
    config: &QuadrantChartConfig,
    theme: &serde_json::Map<String, serde_json::Value>,
    x_axis_position: &str,
    show_x_axis: bool,
    show_y_axis: bool,
    space: &Space,
) -> Vec<TextItem> {
    let quadrant_left = space.quadrant_left;
    let quadrant_top = space.quadrant_top;
    let quadrant_width = space.quadrant_width;
    let quadrant_height = space.quadrant_height;
    let quadrant_half_width = space.quadrant_half_width;
    let quadrant_half_height = space.quadrant_half_height;

    let draw_x_axis_labels_in_middle = !ast.x_axis_right_text.is_empty();
    let draw_y_axis_labels_in_middle = !ast.y_axis_top_text.is_empty();

    let mut axis_labels = Vec::new();

    if !ast.x_axis_left_text.is_empty() && show_x_axis {
        axis_labels.push(TextItem {
            text: ast.x_axis_left_text.clone(),
            fill: theme_string(theme, "quadrantXAxisTextFill").map(str::to_string),
            x: quadrant_left
                + if draw_x_axis_labels_in_middle {
                    quadrant_half_width / 2.0
                } else {
                    0.0
                },
            y: if x_axis_position == "top" {
                config.x_axis_label_padding + space.title_space_top
            } else {
                config.x_axis_label_padding
                    + quadrant_top
                    + quadrant_height
                    + config.quadrant_padding
            },
            font_size: config.x_axis_label_font_size,
            vertical_pos: if draw_x_axis_labels_in_middle {
                TextVerticalPos::Center
            } else {
                TextVerticalPos::Left
            },
            horizontal_pos: TextHorizontalPos::Top,
            rotation: 0.0,
        });
    }
    if !ast.x_axis_right_text.is_empty() && show_x_axis {
        axis_labels.push(TextItem {
            text: ast.x_axis_right_text.clone(),
            fill: theme_string(theme, "quadrantXAxisTextFill").map(str::to_string),
            x: quadrant_left
                + quadrant_half_width
                + if draw_x_axis_labels_in_middle {
                    quadrant_half_width / 2.0
                } else {
                    0.0
                },
            y: if x_axis_position == "top" {
                config.x_axis_label_padding + space.title_space_top
            } else {
                config.x_axis_label_padding
                    + quadrant_top
                    + quadrant_height
                    + config.quadrant_padding
            },
            font_size: config.x_axis_label_font_size,
            vertical_pos: if draw_x_axis_labels_in_middle {
                TextVerticalPos::Center
            } else {
                TextVerticalPos::Left
            },
            horizontal_pos: TextHorizontalPos::Top,
            rotation: 0.0,
        });
    }

    if !ast.y_axis_bottom_text.is_empty() && show_y_axis {
        axis_labels.push(TextItem {
            text: ast.y_axis_bottom_text.clone(),
            fill: theme_string(theme, "quadrantYAxisTextFill").map(str::to_string),
            x: if config.y_axis_position == "left" {
                config.y_axis_label_padding
            } else {
                config.y_axis_label_padding
                    + quadrant_left
                    + quadrant_width
                    + config.quadrant_padding
            },
            y: quadrant_top + quadrant_height
                - if draw_y_axis_labels_in_middle {
                    quadrant_half_height / 2.0
                } else {
                    0.0
                },
            font_size: config.y_axis_label_font_size,
            vertical_pos: if draw_y_axis_labels_in_middle {
                TextVerticalPos::Center
            } else {
                TextVerticalPos::Left
            },
            horizontal_pos: TextHorizontalPos::Top,
            rotation: -90.0,
        });
    }
    if !ast.y_axis_top_text.is_empty() && show_y_axis {
        axis_labels.push(TextItem {
            text: ast.y_axis_top_text.clone(),
            fill: theme_string(theme, "quadrantYAxisTextFill").map(str::to_string),
            x: if config.y_axis_position == "left" {
                config.y_axis_label_padding
            } else {
                config.y_axis_label_padding
                    + quadrant_left
                    + quadrant_width
                    + config.quadrant_padding
            },
            y: quadrant_top + quadrant_half_height
                - if draw_y_axis_labels_in_middle {
                    quadrant_half_height / 2.0
                } else {
                    0.0
                },
            font_size: config.y_axis_label_font_size,
            vertical_pos: if draw_y_axis_labels_in_middle {
                TextVerticalPos::Center
            } else {
                TextVerticalPos::Left
            },
            horizontal_pos: TextHorizontalPos::Top,
            rotation: -90.0,
        });
    }

    axis_labels
}

/// `getQuadrants`: the four boxes, their labels at the top when points exist and centered when
/// they do not.
fn get_quadrants(
    ast: &Ast,
    config: &QuadrantChartConfig,
    theme: &serde_json::Map<String, serde_json::Value>,
    space: &Space,
) -> Vec<QuadrantItem> {
    let quadrant_left = space.quadrant_left;
    let quadrant_top = space.quadrant_top;
    let quadrant_half_width = space.quadrant_half_width;
    let quadrant_half_height = space.quadrant_half_height;

    let label = |text: &str, fill_key: &str| TextItem {
        text: text.to_string(),
        fill: theme_string(theme, fill_key).map(str::to_string),
        x: 0.0,
        y: 0.0,
        font_size: config.quadrant_label_font_size,
        vertical_pos: TextVerticalPos::Center,
        horizontal_pos: TextHorizontalPos::Middle,
        rotation: 0.0,
    };

    let mut quadrants = vec![
        QuadrantItem {
            text: label(&ast.quadrant1_text, "quadrant1TextFill"),
            x: quadrant_left + quadrant_half_width,
            y: quadrant_top,
            width: quadrant_half_width,
            height: quadrant_half_height,
            fill: theme_string(theme, "quadrant1Fill").map(str::to_string),
        },
        QuadrantItem {
            text: label(&ast.quadrant2_text, "quadrant2TextFill"),
            x: quadrant_left,
            y: quadrant_top,
            width: quadrant_half_width,
            height: quadrant_half_height,
            fill: theme_string(theme, "quadrant2Fill").map(str::to_string),
        },
        QuadrantItem {
            text: label(&ast.quadrant3_text, "quadrant3TextFill"),
            x: quadrant_left,
            y: quadrant_top + quadrant_half_height,
            width: quadrant_half_width,
            height: quadrant_half_height,
            fill: theme_string(theme, "quadrant3Fill").map(str::to_string),
        },
        QuadrantItem {
            text: label(&ast.quadrant4_text, "quadrant4TextFill"),
            x: quadrant_left + quadrant_half_width,
            y: quadrant_top + quadrant_half_height,
            width: quadrant_half_width,
            height: quadrant_half_height,
            fill: theme_string(theme, "quadrant4Fill").map(str::to_string),
        },
    ];

    for quadrant in &mut quadrants {
        quadrant.text.x = quadrant.x + quadrant.width / 2.0;
        if ast.points.is_empty() {
            quadrant.text.y = quadrant.y + quadrant.height / 2.0;
            quadrant.text.horizontal_pos = TextHorizontalPos::Middle;
        } else {
            quadrant.text.y = quadrant.y + config.quadrant_text_top_padding;
            quadrant.text.horizontal_pos = TextHorizontalPos::Top;
        }
    }

    quadrants
}

/// `getQuadrantPoints`: map each point's `[0, 1]` pair onto the quadrant square and resolve
/// its class styles, its own styles, and the theme's defaults.
fn get_quadrant_points(
    ast: &Ast,
    config: &QuadrantChartConfig,
    theme: &serde_json::Map<String, serde_json::Value>,
    space: &Space,
) -> Vec<PointItem> {
    let quadrant_left = space.quadrant_left;
    let quadrant_top = space.quadrant_top;
    let quadrant_width = space.quadrant_width;
    let quadrant_height = space.quadrant_height;

    let x_axis = |point: f64| scale_linear(point, quadrant_left, quadrant_width + quadrant_left);
    let y_axis = |point: f64| scale_linear(point, quadrant_height + quadrant_top, quadrant_top);

    ast.points
        .iter()
        .map(|point| {
            // `point = { ...classStyles, ...point }`: the point's own styles win over its
            // class's, and the class only fills in what the point left unset.
            let class_styles = ast.class(&point.class_name);
            let class_radius = class_styles.and_then(|styles| styles.radius);
            let class_color = class_styles.and_then(|styles| styles.color.as_deref());
            let class_stroke_color = class_styles.and_then(|styles| styles.stroke_color.as_deref());
            let class_stroke_width = class_styles.and_then(|styles| styles.stroke_width.as_deref());

            let x = x_axis(point_coordinate(&point.x));
            let y = y_axis(point_coordinate(&point.y));

            PointItem {
                x,
                y,
                fill: point
                    .styles
                    .color
                    .as_deref()
                    .or(class_color)
                    .or_else(|| theme_string(theme, "quadrantPointFill"))
                    .map(str::to_string),
                radius: point
                    .styles
                    .radius
                    .or(class_radius)
                    .unwrap_or(config.point_radius),
                text: TextItem {
                    text: point.text.clone(),
                    fill: theme_string(theme, "quadrantPointTextFill").map(str::to_string),
                    x,
                    y: y + config.point_text_padding,
                    vertical_pos: TextVerticalPos::Center,
                    horizontal_pos: TextHorizontalPos::Top,
                    font_size: config.point_label_font_size,
                    rotation: 0.0,
                },
                stroke_color: point
                    .styles
                    .stroke_color
                    .as_deref()
                    .or(class_stroke_color)
                    .or_else(|| theme_string(theme, "quadrantPointFill"))
                    .map(str::to_string),
                stroke_width: stroke_width(
                    point.styles.stroke_width.as_deref().or(class_stroke_width),
                ),
            }
        })
        .collect()
}

/// `getBorders`: the four outer edges (extended by half their stroke width at the corners) and
/// the two inner dividers.
fn get_borders(
    config: &QuadrantChartConfig,
    theme: &serde_json::Map<String, serde_json::Value>,
    space: &Space,
) -> Vec<LineItem> {
    let half_external_border_width = config.quadrant_external_border_stroke_width / 2.0;
    let quadrant_left = space.quadrant_left;
    let quadrant_top = space.quadrant_top;
    let quadrant_width = space.quadrant_width;
    let quadrant_height = space.quadrant_height;
    let quadrant_half_width = space.quadrant_half_width;
    let quadrant_half_height = space.quadrant_half_height;

    let external = || theme_string(theme, "quadrantExternalBorderStrokeFill").map(str::to_string);
    let internal = || theme_string(theme, "quadrantInternalBorderStrokeFill").map(str::to_string);

    vec![
        LineItem {
            stroke_fill: external(),
            stroke_width: config.quadrant_external_border_stroke_width,
            x1: quadrant_left - half_external_border_width,
            y1: quadrant_top,
            x2: quadrant_left + quadrant_width + half_external_border_width,
            y2: quadrant_top,
        },
        LineItem {
            stroke_fill: external(),
            stroke_width: config.quadrant_external_border_stroke_width,
            x1: quadrant_left + quadrant_width,
            y1: quadrant_top + half_external_border_width,
            x2: quadrant_left + quadrant_width,
            y2: quadrant_top + quadrant_height - half_external_border_width,
        },
        LineItem {
            stroke_fill: external(),
            stroke_width: config.quadrant_external_border_stroke_width,
            x1: quadrant_left - half_external_border_width,
            y1: quadrant_top + quadrant_height,
            x2: quadrant_left + quadrant_width + half_external_border_width,
            y2: quadrant_top + quadrant_height,
        },
        LineItem {
            stroke_fill: external(),
            stroke_width: config.quadrant_external_border_stroke_width,
            x1: quadrant_left,
            y1: quadrant_top + half_external_border_width,
            x2: quadrant_left,
            y2: quadrant_top + quadrant_height - half_external_border_width,
        },
        LineItem {
            stroke_fill: internal(),
            stroke_width: config.quadrant_internal_border_stroke_width,
            x1: quadrant_left + quadrant_half_width,
            y1: quadrant_top + half_external_border_width,
            x2: quadrant_left + quadrant_half_width,
            y2: quadrant_top + quadrant_height - half_external_border_width,
        },
        LineItem {
            stroke_fill: internal(),
            stroke_width: config.quadrant_internal_border_stroke_width,
            x1: quadrant_left + half_external_border_width,
            y1: quadrant_top + quadrant_half_height,
            x2: quadrant_left + quadrant_width - half_external_border_width,
            y2: quadrant_top + quadrant_half_height,
        },
    ]
}

/// `getTitle`: the title centered at the top, when the diagram has one.
fn get_title(
    ast: &Ast,
    config: &QuadrantChartConfig,
    theme: &serde_json::Map<String, serde_json::Value>,
    show_title: bool,
) -> Option<TextItem> {
    show_title.then(|| TextItem {
        text: ast.diagram_title.clone(),
        fill: theme_string(theme, "quadrantTitleFill").map(str::to_string),
        font_size: config.title_font_size,
        horizontal_pos: TextHorizontalPos::Top,
        vertical_pos: TextVerticalPos::Center,
        rotation: 0.0,
        y: config.title_padding,
        x: config.chart_width / 2.0,
    })
}

/// One text element: a [`TextRun`] positioned by `translate(x, y) rotate(rotation)` with the
/// label's SVG baseline and anchor.
fn text_run(item: &TextItem, measure: &dyn TextMeasure) -> TextRun {
    let font = Font::new("monospace", item.font_size);
    let options = TextOptions {
        width: f64::INFINITY,
        markdown: false,
        min_width: 0.0,
    };
    let block = shape_text(&item.text, &options, &font, measure).unwrap_or_default();
    let mut run = TextRun::new(ScenePoint::new(item.x, item.y), font, block);
    run.anchor = text_anchor(item.vertical_pos);
    run.baseline = dominant_baseline(item.horizontal_pos);
    run.rotation = item.rotation;
    run.fill = item.fill.as_deref().map(Color::new);
    run
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Item, PathCommand};

    const TOLERANCE: f64 = 0.01;

    fn fixtures() -> Vec<(String, serde_json::Value)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/diagrams/quadrant_chart/tests/render");
        let mut fixtures: Vec<(String, serde_json::Value)> = std::fs::read_dir(&dir)
            .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
            .map(|entry| entry.expect("dir entry").path())
            .filter(|path| {
                path.extension().and_then(|extension| extension.to_str()) == Some("json")
            })
            .map(|path| {
                let text = std::fs::read_to_string(&path).expect("read fixture");
                let value: serde_json::Value = serde_json::from_str(&text).expect("parse fixture");
                (
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or_default()
                        .to_string(),
                    value,
                )
            })
            .collect();
        fixtures.sort_by(|a, b| a.0.cmp(&b.0));
        fixtures
    }

    fn close(actual: f64, expected: f64, what: &str, file: &str) {
        assert!(
            (actual - expected).abs() <= TOLERANCE,
            "{file}: {what}: actual {actual} vs expected {expected}"
        );
    }

    fn expected_number(expected: &serde_json::Value, key: &str, what: &str, file: &str) -> f64 {
        expected[key]
            .as_f64()
            .unwrap_or_else(|| panic!("{file}: {what} has no numeric {key}: {expected}"))
    }

    fn group<'a>(item: &'a Item, file: &str, what: &str) -> &'a Group {
        match item {
            Item::Group(group) => group,
            _ => panic!("{file}: {what} is not a group"),
        }
    }

    fn text<'a>(item: &'a Item, file: &str, what: &str) -> &'a TextRun {
        match item {
            Item::Text(text) => text,
            _ => panic!("{file}: {what} is not text"),
        }
    }

    fn text_of(run: &TextRun) -> String {
        run.lines
            .iter()
            .map(|line| line.text())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn group_class<'a>(parent: &'a Group, class: &str, file: &str) -> &'a Group {
        match parent.items.iter().find(
            |item| matches!(item, Item::Group(child) if child.class.as_deref() == Some(class)),
        ) {
            Some(Item::Group(group)) => group,
            _ => panic!("{file}: no {class} group"),
        }
    }

    fn anchor_name(anchor: TextAnchor) -> &'static str {
        match anchor {
            TextAnchor::Start => "start",
            TextAnchor::Middle => "middle",
            TextAnchor::End => "end",
        }
    }

    fn baseline_name(baseline: TextBaseline) -> &'static str {
        match baseline {
            TextBaseline::Alphabetic => "auto",
            TextBaseline::Middle => "middle",
            TextBaseline::Central => "central",
            TextBaseline::Hanging => "hanging",
            TextBaseline::Top => "top",
            TextBaseline::Bottom => "bottom",
        }
    }

    fn assert_text(run: &TextRun, expected: &serde_json::Value, file: &str, what: &str) {
        assert_eq!(
            text_of(run),
            expected["text"].as_str().expect("text"),
            "{file}: {what} text"
        );
        close(
            run.position.x,
            expected_number(expected, "x", what, file),
            "x",
            file,
        );
        close(
            run.position.y,
            expected_number(expected, "y", what, file),
            "y",
            file,
        );
        close(
            run.rotation,
            expected_number(expected, "rotation", what, file),
            "rotation",
            file,
        );
        close(
            run.font.size,
            expected_number(expected, "fontSize", what, file),
            "font size",
            file,
        );
        close(
            run.size.width,
            expected_number(expected, "width", what, file),
            "width",
            file,
        );
        close(
            run.size.height,
            expected_number(expected, "height", what, file),
            "height",
            file,
        );
        assert_eq!(
            anchor_name(run.anchor),
            expected["anchor"].as_str().expect("anchor"),
            "{file}: {what} anchor"
        );
        assert_eq!(
            baseline_name(run.baseline),
            expected["baseline"].as_str().expect("baseline"),
            "{file}: {what} baseline"
        );
        assert_eq!(
            run.fill.as_ref().map(|color| color.0.as_str()),
            expected["fill"].as_str(),
            "{file}: {what} fill"
        );
    }

    /// Renders a fixture's source with the config its directives produce.
    fn scene_of(fixture: &serde_json::Value) -> Scene {
        let source = fixture["source"].as_str().expect("fixture source");
        let preprocessed = crate::preprocess::preprocess(source);
        let config = Config::from_source_config(&preprocessed.config)
            .unwrap_or_else(|error| panic!("config: {error}"));
        let ast = super::super::parse(&preprocessed.body)
            .unwrap_or_else(|error| panic!("parse: {error}"));
        draw_ast(&ast, &config, &crate::measure::FallbackMeasure)
    }

    #[test]
    fn renders_recorded_quadrant_fixtures() {
        let fixtures = fixtures();
        assert!(
            fixtures.len() >= 8,
            "expected at least 8 fixtures, found {}",
            fixtures.len()
        );

        for (file, fixture) in fixtures {
            let scene = scene_of(&fixture);
            let expected = &fixture["expected"];

            close(
                scene.view_box.x,
                expected_number(&expected["viewBox"], "x", "viewBox", &file),
                "viewBox x",
                &file,
            );
            close(
                scene.view_box.y,
                expected_number(&expected["viewBox"], "y", "viewBox", &file),
                "viewBox y",
                &file,
            );
            close(
                scene.view_box.width,
                expected_number(&expected["viewBox"], "width", "viewBox", &file),
                "viewBox width",
                &file,
            );
            close(
                scene.view_box.height,
                expected_number(&expected["viewBox"], "height", "viewBox", &file),
                "viewBox height",
                &file,
            );

            let main = group(&scene.items[0], &file, "main");
            assert_eq!(main.class.as_deref(), Some("main"), "{file}: main class");

            let quadrants = group_class(main, "quadrants", &file);
            let expected_quadrants = expected["quadrants"].as_array().expect("quadrants");
            assert_eq!(
                quadrants.items.len(),
                expected_quadrants.len(),
                "{file}: quadrant count"
            );
            for (index, (item, expected)) in
                quadrants.items.iter().zip(expected_quadrants).enumerate()
            {
                let quadrant = group(item, &file, &format!("quadrant {index}"));
                assert_eq!(
                    quadrant.class.as_deref(),
                    Some("quadrant"),
                    "{file}: quadrant {index} class"
                );
                let rect = match &quadrant.items[0] {
                    Item::Rect(rect) => rect,
                    _ => panic!("{file}: quadrant {index} is not a rect"),
                };
                close(
                    rect.rect.x,
                    expected_number(&expected["rect"], "x", "quadrant rect", &file),
                    &format!("quadrant {index} x"),
                    &file,
                );
                close(
                    rect.rect.y,
                    expected_number(&expected["rect"], "y", "quadrant rect", &file),
                    &format!("quadrant {index} y"),
                    &file,
                );
                close(
                    rect.rect.width,
                    expected_number(&expected["rect"], "width", "quadrant rect", &file),
                    &format!("quadrant {index} width"),
                    &file,
                );
                close(
                    rect.rect.height,
                    expected_number(&expected["rect"], "height", "quadrant rect", &file),
                    &format!("quadrant {index} height"),
                    &file,
                );
                assert_eq!(
                    rect.style.fill.as_ref().map(|color| color.0.as_str()),
                    expected["rect"]["fill"].as_str(),
                    "{file}: quadrant {index} fill"
                );
                assert_text(
                    text(&quadrant.items[1], &file, "quadrant text"),
                    &expected["text"],
                    &file,
                    &format!("quadrant {index} text"),
                );
            }

            let border = group_class(main, "border", &file);
            let expected_lines = expected["borderLines"].as_array().expect("border lines");
            assert_eq!(
                border.items.len(),
                expected_lines.len(),
                "{file}: border line count"
            );
            for (index, (item, expected)) in border.items.iter().zip(expected_lines).enumerate() {
                let line = match item {
                    Item::Path(line) => line,
                    _ => panic!("{file}: border {index} is not a path"),
                };
                match line.path.commands.as_slice() {
                    [
                        PathCommand::MoveTo { x, y },
                        PathCommand::LineTo { x: x2, y: y2 },
                    ] => {
                        close(
                            *x,
                            expected_number(expected, "x1", "border", &file),
                            &format!("border {index} x1"),
                            &file,
                        );
                        close(
                            *y,
                            expected_number(expected, "y1", "border", &file),
                            &format!("border {index} y1"),
                            &file,
                        );
                        close(
                            *x2,
                            expected_number(expected, "x2", "border", &file),
                            &format!("border {index} x2"),
                            &file,
                        );
                        close(
                            *y2,
                            expected_number(expected, "y2", "border", &file),
                            &format!("border {index} y2"),
                            &file,
                        );
                    }
                    commands => panic!("{file}: border {index} has commands {commands:?}"),
                }
                assert_eq!(
                    line.style.stroke.as_ref().map(|color| color.0.as_str()),
                    expected["stroke"].as_str(),
                    "{file}: border {index} stroke"
                );
                close(
                    line.style.stroke_width,
                    expected_number(expected, "strokeWidth", "border", &file),
                    &format!("border {index} stroke width"),
                    &file,
                );
            }

            let data_points = group_class(main, "data-points", &file);
            let expected_points = expected["points"].as_array().expect("points");
            assert_eq!(
                data_points.items.len(),
                expected_points.len(),
                "{file}: point count"
            );
            for (index, (item, expected)) in
                data_points.items.iter().zip(expected_points).enumerate()
            {
                let point = group(item, &file, &format!("point {index}"));
                assert_eq!(
                    point.class.as_deref(),
                    Some("data-point"),
                    "{file}: point {index} class"
                );
                let circle = match &point.items[0] {
                    Item::Ellipse(circle) => circle,
                    _ => panic!("{file}: point {index} is not an ellipse"),
                };
                close(
                    circle.center.x,
                    expected_number(&expected["circle"], "cx", "point circle", &file),
                    &format!("point {index} cx"),
                    &file,
                );
                close(
                    circle.center.y,
                    expected_number(&expected["circle"], "cy", "point circle", &file),
                    &format!("point {index} cy"),
                    &file,
                );
                close(
                    circle.radius_x,
                    expected_number(&expected["circle"], "r", "point circle", &file),
                    &format!("point {index} r"),
                    &file,
                );
                assert_eq!(
                    circle.radius_x, circle.radius_y,
                    "{file}: point {index} radius"
                );
                assert_eq!(
                    circle.style.fill.as_ref().map(|color| color.0.as_str()),
                    expected["circle"]["fill"].as_str(),
                    "{file}: point {index} fill"
                );
                assert_eq!(
                    circle.style.stroke.as_ref().map(|color| color.0.as_str()),
                    expected["circle"]["stroke"].as_str(),
                    "{file}: point {index} stroke"
                );
                close(
                    circle.style.stroke_width,
                    expected_number(&expected["circle"], "strokeWidth", "point circle", &file),
                    &format!("point {index} stroke width"),
                    &file,
                );
                assert_text(
                    text(&point.items[1], &file, "point text"),
                    &expected["text"],
                    &file,
                    &format!("point {index} text"),
                );
            }

            let labels = group_class(main, "labels", &file);
            let expected_labels = expected["axisLabels"].as_array().expect("axis labels");
            assert_eq!(
                labels.items.len(),
                expected_labels.len(),
                "{file}: axis label count"
            );
            for (index, (item, expected)) in labels.items.iter().zip(expected_labels).enumerate() {
                let label = group(item, &file, &format!("axis label {index}"));
                assert_eq!(
                    label.class.as_deref(),
                    Some("label"),
                    "{file}: axis label {index} class"
                );
                assert_text(
                    text(&label.items[0], &file, "axis label text"),
                    expected,
                    &file,
                    &format!("axis label {index}"),
                );
            }

            match &expected["title"] {
                serde_json::Value::Null => assert!(
                    !main.items.iter().any(
                        |item| matches!(item, Item::Group(group) if group.class.as_deref() == Some("title"))
                    ),
                    "{file}: unexpected title group"
                ),
                expected_title => {
                    let title_group = group_class(main, "title", &file);
                    assert_text(
                        text(&title_group.items[0], &file, "title"),
                        expected_title,
                        &file,
                        "title",
                    );
                }
            }
        }
    }

    #[test]
    fn styles_the_title_and_labels_from_the_default_theme() {
        let config = Config::default();
        let scene = render(
            "quadrantChart\n    title Theme\n    x-axis Low --> High\n    y-axis Low --> High\n    quadrant-1 a\n    quadrant-2 b\n    quadrant-3 c\n    quadrant-4 d\n    P: [0.5, 0.5]\n",
            &config,
        )
        .expect("render quadrant chart");
        let theme = &config.theme_variables;
        let theme_value = |key: &str| theme[key].as_str().expect("theme string").to_string();

        let main = group(&scene.items[0], "scene", "main");
        let title_group = group_class(main, "title", "scene");
        let title = text(&title_group.items[0], "scene", "title");
        assert_eq!(
            title.fill.as_ref().map(|color| color.0.clone()),
            Some(theme_value("quadrantTitleFill"))
        );

        let quadrants = group_class(main, "quadrants", "scene");
        for (index, item) in quadrants.items.iter().enumerate() {
            let quadrant = group(item, "scene", "quadrant");
            let rect = match &quadrant.items[0] {
                Item::Rect(rect) => rect,
                _ => panic!("quadrant is not a rect"),
            };
            assert_eq!(
                rect.style.fill.as_ref().map(|color| color.0.clone()),
                Some(theme_value(&format!("quadrant{}Fill", index + 1)))
            );
            let label = text(&quadrant.items[1], "scene", "quadrant text");
            assert_eq!(
                label.fill.as_ref().map(|color| color.0.clone()),
                Some(theme_value(&format!("quadrant{}TextFill", index + 1)))
            );
        }

        let border = group_class(main, "border", "scene");
        let stroke = border
            .items
            .iter()
            .map(|item| match item {
                Item::Path(path) => path
                    .style
                    .stroke
                    .as_ref()
                    .map(|color| color.0.clone())
                    .expect("border stroke"),
                _ => panic!("border is not a path"),
            })
            .collect::<Vec<_>>();
        assert_eq!(stroke[0], theme_value("quadrantExternalBorderStrokeFill"));
        assert_eq!(stroke[4], theme_value("quadrantInternalBorderStrokeFill"));

        let points = group_class(main, "data-points", "scene");
        let point = group(&points.items[0], "scene", "point");
        let circle = match &point.items[0] {
            Item::Ellipse(circle) => circle,
            _ => panic!("point is not an ellipse"),
        };
        assert_eq!(
            circle.style.fill.as_ref().map(|color| color.0.clone()),
            Some(theme_value("quadrantPointFill"))
        );
        let label = text(&point.items[1], "scene", "point text");
        assert_eq!(
            label.fill.as_ref().map(|color| color.0.clone()),
            Some(theme_value("quadrantPointTextFill"))
        );
    }

    #[test]
    fn render_diagram_dispatches_quadrant_charts() {
        let scene = crate::render_diagram(
            "quadrantChart\n    title Dispatch\n    quadrant-1 a\n    quadrant-2 b\n    quadrant-3 c\n    quadrant-4 d\n",
            &Config::default(),
        )
        .expect("render quadrant chart");
        let main = group(&scene.items[0], "scene", "main");
        assert_eq!(main.class.as_deref(), Some("main"));
        let title_group = group_class(main, "title", "scene");
        assert_eq!(
            text_of(text(&title_group.items[0], "scene", "title")),
            "Dispatch"
        );
    }

    #[test]
    fn rejects_a_body_without_a_quadrant_header() {
        let error = render("title nope\n", &Config::default()).unwrap_err();
        assert!(
            error.message.contains("QUADRANT"),
            "unexpected message: {}",
            error.message
        );
    }
}
