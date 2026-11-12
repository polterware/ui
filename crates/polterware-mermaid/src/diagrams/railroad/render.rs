//! Source-to-[`Scene`] rendering for railroad (syntax) diagrams: the Rust side of
//! `railroadRenderer.ts` `draw`, with `styles.ts`'s options.
//!
//! A railroad diagram does not go through the unified layout pipeline: `draw` appends shapes to
//! the SVG with plain d3, measuring each label by a temporary `<text>`'s `getBBox()`. This module
//! ports that renderer line for line. Each rule is a group at `translate(0, y)` holding the rule
//! name, the definition's expression tree, a filled start and end marker, and the two connecting
//! segments. An expression is one of seven nodes, every one laid out by its own recursive
//! routine: a rounded terminal box, a square non-terminal box, a dashed `? ... ?` special box, a
//! horizontal sequence, a vertical choice joined by arcs, an optional's upper bypass, and a
//! repetition's lower loop (with an upper bypass for `*`).
//!
//! The dimensions are the ones the classes in `styles.ts` draw: a terminal's box is its text
//! plus `padding` on every side and a fixed 10-unit corner radius, a choice spends
//! `arcRadius * 4` on its two curved columns, an optional and a repetition spend the same on
//! their paths, and the whole drawing is the widest rule plus `padding` on each side. The
//! viewport is `0 0 width height`, which is what `configureRailroadSvgSize` sets after
//! `configureSvgSize`.
//!
//! Text is measured with the [`TextMeasure`] handed in, using the family and size the style
//! options resolve (the theme's `fontFamily`/`fontSize` unless the `railroad` config overrides
//! them); [`render`] uses the crate's monospace [`FallbackMeasure`](crate::measure::FallbackMeasure).
//! Colors come from the theme variables through `buildRailroadStyleOptions`: the terminal box
//! takes `secondBkg`/`secondaryBorderColor`/`secondaryTextColor`, the non-terminal box
//! `mainBkg`/`primaryBorderColor`/`primaryTextColor`, the special box `tertiaryColor`/
//! `tertiaryBorderColor`, the lines `lineColor`, the markers `lineColor`, and the rule name
//! `titleColor`, each falling back to `DEFAULT_RAILROAD_CONFIG` when the theme omits it. The
//! `railroad` config's own colors, sizes, and spacing override those.
//!
//! There is no gpui-kit counterpart to match: gpui-kit has no railroad diagram, so the
//! behavior is upstream's own, including its quirks. `RailroadRule.comment` is never drawn
//! (the grammar never sets it), `showMarkers` and `compactMode` are read into the options but
//! never consulted (the renderer always draws markers and has no compact branch), the
//! repetition's `separator` is never drawn, and a `choice` whose alternative sits exactly on
//! the centre line gets straight segments instead of the two arcs. An empty rule list draws
//! nothing and reports the 200 × 100 viewport `draw` sets for it.
//!
//! The class names (`railroad-terminal`, `railroad-line`, ...) matter only to CSS, and a
//! [`Scene`] carries paints rather than selectors, so `styles.ts`' declarations are resolved
//! into the shapes here.

use std::collections::HashMap;

use serde_json::Value;

use crate::config::{Config, RailroadConfig};
use crate::error::ParseError;
use crate::measure::{Font, TextMeasure};
use crate::render::shapes::label_image_utils::parse_font_size;
use crate::scene::{
    BOLD_WEIGHT, Color, EllipseShape, Group, Path, PathShape, PathStyle, Point, Rect, RectShape,
    Scene, Size, TextAnchor, TextBaseline, TextBlock, TextLine, TextRun, TextSpan, Transform,
};

use super::{Ast, Node, Rule};

/// `DEFAULT_RAILROAD_CONFIG.fontFamily`.
const DEFAULT_FONT_FAMILY: &str = "monospace";
/// `DEFAULT_RAILROAD_CONFIG.fontSize`.
const DEFAULT_FONT_SIZE: f64 = 14.0;
/// `DEFAULT_RAILROAD_CONFIG.padding`.
const DEFAULT_PADDING: f64 = 10.0;
/// `DEFAULT_RAILROAD_CONFIG.verticalSeparation`.
const DEFAULT_VERTICAL_SEPARATION: f64 = 8.0;
/// `DEFAULT_RAILROAD_CONFIG.horizontalSeparation`.
const DEFAULT_HORIZONTAL_SEPARATION: f64 = 10.0;
/// `DEFAULT_RAILROAD_CONFIG.arcRadius`.
const DEFAULT_ARC_RADIUS: f64 = 10.0;
/// `DEFAULT_RAILROAD_CONFIG.strokeWidth`.
const DEFAULT_STROKE_WIDTH: f64 = 2.0;
/// `DEFAULT_RAILROAD_CONFIG.markerRadius`.
const DEFAULT_MARKER_RADIUS: f64 = 5.0;
/// `DEFAULT_RAILROAD_CONFIG.terminalFill`.
const DEFAULT_TERMINAL_FILL: &str = "#FFFFC0";
/// `DEFAULT_RAILROAD_CONFIG.terminalStroke`.
const DEFAULT_TERMINAL_STROKE: &str = "#000000";
/// `DEFAULT_RAILROAD_CONFIG.terminalTextColor`.
const DEFAULT_TERMINAL_TEXT_COLOR: &str = "#000000";
/// `DEFAULT_RAILROAD_CONFIG.nonTerminalFill`.
const DEFAULT_NON_TERMINAL_FILL: &str = "#FFFFFF";
/// `DEFAULT_RAILROAD_CONFIG.nonTerminalStroke`.
const DEFAULT_NON_TERMINAL_STROKE: &str = "#000000";
/// `DEFAULT_RAILROAD_CONFIG.nonTerminalTextColor`.
const DEFAULT_NON_TERMINAL_TEXT_COLOR: &str = "#000000";
/// `DEFAULT_RAILROAD_CONFIG.lineColor`.
const DEFAULT_LINE_COLOR: &str = "#000000";
/// `DEFAULT_RAILROAD_CONFIG.markerFill`.
const DEFAULT_MARKER_FILL: &str = "#000000";
/// `DEFAULT_RAILROAD_CONFIG.specialFill`.
const DEFAULT_SPECIAL_FILL: &str = "#F0E0FF";
/// `DEFAULT_RAILROAD_CONFIG.specialStroke`.
const DEFAULT_SPECIAL_STROKE: &str = "#8800CC";
/// `DEFAULT_RAILROAD_CONFIG.ruleNameColor`.
const DEFAULT_RULE_NAME_COLOR: &str = "#000066";

/// `renderTerminal`'s hard-coded corner radius.
const TERMINAL_CORNER_RADIUS: f64 = 10.0;
/// The gap between the rule name and the start marker, `renderRule`'s `+ 20`.
const RULE_NAME_GAP: f64 = 20.0;
/// The gap between the definition and the end marker, `renderRule`'s `+ 10`.
const DEFINITION_END_GAP: f64 = 10.0;
/// `renderRule`'s `Math.max(20, result.dimensions.up)` baseline floor.
const MIN_BASELINE: f64 = 20.0;
/// `renderRule`'s `Math.max(40, ...)` rule height floor.
const MIN_RULE_HEIGHT: f64 = 40.0;
/// `draw`'s viewport for a diagram with no rules.
const EMPTY_VIEW_WIDTH: f64 = 200.0;
/// `draw`'s viewport for a diagram with no rules.
const EMPTY_VIEW_HEIGHT: f64 = 100.0;

/// `buildRailroadStyleOptions`: the theme-derived defaults with the `railroad` config on top.
#[derive(Clone, Debug)]
struct StyleOptions {
    padding: f64,
    vertical_separation: f64,
    horizontal_separation: f64,
    arc_radius: f64,
    font_size: f64,
    font_family: String,
    terminal_fill: String,
    terminal_stroke: String,
    terminal_text_color: String,
    non_terminal_fill: String,
    non_terminal_stroke: String,
    non_terminal_text_color: String,
    line_color: String,
    stroke_width: f64,
    marker_fill: String,
    special_fill: String,
    special_stroke: String,
    rule_name_color: String,
    marker_radius: f64,
}

impl StyleOptions {
    fn from_config(config: &Config) -> Self {
        let theme = &config.theme_variables;
        let theme_text = |keys: &[&str]| -> Option<&str> {
            keys.iter()
                .find_map(|key| theme.get(*key).and_then(Value::as_str))
        };

        // `buildThemeDefaults`: the theme's typography and colors, each sanitized with the
        // hard-coded default as the fallback.
        let theme_font_family =
            sanitize_font_family(theme_text(&["fontFamily"]), DEFAULT_FONT_FAMILY);
        let theme_font_size = theme_text(&["fontSize"])
            .and_then(parse_font_size)
            .filter(|size| *size > 0.0)
            .unwrap_or(DEFAULT_FONT_SIZE);
        let theme_terminal_fill = sanitize_color(
            theme_text(&["secondBkg", "secondaryColor"]),
            DEFAULT_TERMINAL_FILL,
        );
        let theme_terminal_stroke = sanitize_color(
            theme_text(&["secondaryBorderColor", "lineColor"]),
            DEFAULT_TERMINAL_STROKE,
        );
        let theme_terminal_text = sanitize_color(
            theme_text(&["secondaryTextColor", "textColor"]),
            DEFAULT_TERMINAL_TEXT_COLOR,
        );
        let theme_non_terminal_fill = sanitize_color(
            theme_text(&["mainBkg", "background"]),
            DEFAULT_NON_TERMINAL_FILL,
        );
        let theme_non_terminal_stroke = sanitize_color(
            theme_text(&["primaryBorderColor", "lineColor"]),
            DEFAULT_NON_TERMINAL_STROKE,
        );
        let theme_non_terminal_text = sanitize_color(
            theme_text(&["primaryTextColor", "textColor"]),
            DEFAULT_NON_TERMINAL_TEXT_COLOR,
        );
        let theme_line = sanitize_color(theme_text(&["lineColor"]), DEFAULT_LINE_COLOR);
        let theme_marker = sanitize_color(theme_text(&["lineColor"]), DEFAULT_MARKER_FILL);
        let theme_special_fill = sanitize_color(
            theme_text(&["tertiaryColor", "secondaryColor"]),
            DEFAULT_SPECIAL_FILL,
        );
        let theme_special_stroke = sanitize_color(
            theme_text(&["tertiaryBorderColor", "secondaryBorderColor"]),
            DEFAULT_SPECIAL_STROKE,
        );
        let theme_rule_name = sanitize_color(
            theme_text(&["titleColor", "textColor"]),
            DEFAULT_RULE_NAME_COLOR,
        );

        // `buildRailroadStyleOptions`: the config's own values over the theme defaults.
        let railroad: &RailroadConfig = &config.railroad;
        Self {
            padding: sanitize_number(Some(railroad.padding), DEFAULT_PADDING),
            vertical_separation: sanitize_number(
                Some(railroad.vertical_separation),
                DEFAULT_VERTICAL_SEPARATION,
            ),
            horizontal_separation: sanitize_number(
                Some(railroad.horizontal_separation),
                DEFAULT_HORIZONTAL_SEPARATION,
            ),
            arc_radius: sanitize_number(Some(railroad.arc_radius), DEFAULT_ARC_RADIUS),
            font_size: sanitize_number(railroad.font_size, theme_font_size),
            font_family: sanitize_font_family(railroad.font_family.as_deref(), &theme_font_family),
            terminal_fill: sanitize_color(railroad.terminal_fill.as_deref(), &theme_terminal_fill),
            terminal_stroke: sanitize_color(
                railroad.terminal_stroke.as_deref(),
                &theme_terminal_stroke,
            ),
            terminal_text_color: sanitize_color(
                railroad.terminal_text_color.as_deref(),
                &theme_terminal_text,
            ),
            non_terminal_fill: sanitize_color(
                railroad.non_terminal_fill.as_deref(),
                &theme_non_terminal_fill,
            ),
            non_terminal_stroke: sanitize_color(
                railroad.non_terminal_stroke.as_deref(),
                &theme_non_terminal_stroke,
            ),
            non_terminal_text_color: sanitize_color(
                railroad.non_terminal_text_color.as_deref(),
                &theme_non_terminal_text,
            ),
            line_color: sanitize_color(railroad.line_color.as_deref(), &theme_line),
            stroke_width: sanitize_number(Some(railroad.stroke_width), DEFAULT_STROKE_WIDTH),
            marker_fill: sanitize_color(railroad.marker_fill.as_deref(), &theme_marker),
            special_fill: sanitize_color(railroad.special_fill.as_deref(), &theme_special_fill),
            special_stroke: sanitize_color(
                railroad.special_stroke.as_deref(),
                &theme_special_stroke,
            ),
            rule_name_color: sanitize_color(railroad.rule_name_color.as_deref(), &theme_rule_name),
            marker_radius: sanitize_number(Some(railroad.marker_radius), DEFAULT_MARKER_RADIUS),
        }
    }

    fn font(&self) -> Font {
        Font::new(self.font_family.clone(), self.font_size)
    }
}

/// `sanitizeColorValue`: a CSS color or the fallback.
fn sanitize_color(value: Option<&str>, fallback: &str) -> String {
    match value {
        Some(value) if is_color_value(value.trim()) => value.trim().to_string(),
        _ => fallback.to_string(),
    }
}

/// `COLOR_VALUE_PATTERN`, without a regular expression engine.
fn is_color_value(value: &str) -> bool {
    if let Some(hex) = value.strip_prefix('#') {
        return matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|ch| ch.is_ascii_hexdigit());
    }
    let lower = value.to_ascii_lowercase();
    for prefix in [
        "rgb(", "rgba(", "hsl(", "hsla(", "hwb(", "lab(", "lch(", "oklab(", "oklch(",
    ] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let Some(inner) = rest.strip_suffix(')') else {
                return false;
            };
            return !inner.is_empty()
                && inner
                    .chars()
                    .all(|ch| ch.is_ascii_digit() || ch.is_whitespace() || "%+,./-".contains(ch));
        }
    }
    !value.is_empty() && value.chars().all(|ch| ch.is_ascii_alphabetic())
}

/// `sanitizeFontFamilyValue`: a family name or the fallback.
fn sanitize_font_family(value: Option<&str>, fallback: &str) -> String {
    match value {
        Some(value) if is_font_family_value(value.trim()) => value.trim().to_string(),
        _ => fallback.to_string(),
    }
}

/// `FONT_FAMILY_PATTERN`, without a regular expression engine.
fn is_font_family_value(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_alphanumeric() || " \"',.-_".contains(ch))
}

/// `sanitizeNumberValue`: a finite, non-negative number or the fallback.
fn sanitize_number(value: Option<f64>, fallback: f64) -> f64 {
    match value {
        Some(value) if value.is_finite() && value >= 0.0 => value,
        _ => fallback,
    }
}

/// `DiagramDimensions`: the box an expression fills, measured from its centre line.
#[derive(Clone, Copy, Debug)]
struct Dimensions {
    width: f64,
    height: f64,
    up: f64,
    down: f64,
}

/// `RenderResult`: an expression's group and the dimensions it reports.
struct RenderResult {
    group: Group,
    dimensions: Dimensions,
}

/// The anchor, baseline, weight, and fill a label is drawn with.
#[derive(Clone, Copy)]
struct LabelStyle<'a> {
    anchor: TextAnchor,
    baseline: TextBaseline,
    bold: bool,
    fill: &'a str,
}

impl<'a> LabelStyle<'a> {
    /// The box labels: `text-anchor: middle`, `dominant-baseline: middle`, no weight change.
    fn boxed(fill: &'a str) -> Self {
        Self {
            anchor: TextAnchor::Middle,
            baseline: TextBaseline::Middle,
            bold: false,
            fill,
        }
    }

    /// `.railroad-rule-name`: start-anchored on the baseline, in bold.
    fn rule_name(fill: &'a str) -> Self {
        Self {
            anchor: TextAnchor::Start,
            baseline: TextBaseline::Alphabetic,
            bold: true,
            fill,
        }
    }
}

/// The renderer, holding `styles.ts`' resolved options and the `measureText` cache.
struct RailroadRenderer<'a> {
    options: StyleOptions,
    measure: &'a dyn TextMeasure,
    text_cache: HashMap<String, (f64, f64)>,
}

impl<'a> RailroadRenderer<'a> {
    fn new(options: StyleOptions, measure: &'a dyn TextMeasure) -> Self {
        Self {
            options,
            measure,
            text_cache: HashMap::new(),
        }
    }

    /// `measureText`: the `getBBox()` of a one-line `<text>` in the options' font.
    ///
    /// The harness's shared rule gives an empty string a `0 × 0` box, as a browser does.
    fn measure_text(&mut self, text: &str) -> (f64, f64) {
        if let Some(dimensions) = self.text_cache.get(text) {
            return *dimensions;
        }
        let dimensions = if text.is_empty() {
            (0.0, 0.0)
        } else {
            let font = self.options.font();
            (
                self.measure.width(text, &font),
                self.measure.line_height(&font),
            )
        };
        self.text_cache.insert(text.to_string(), dimensions);
        dimensions
    }

    /// A one-line text run with `styles.ts`' fill.
    fn text_run(
        &self,
        text: &str,
        position: Point,
        style: LabelStyle<'_>,
        size: (f64, f64),
    ) -> TextRun {
        let mut font = self.options.font();
        if style.bold {
            font.weight = BOLD_WEIGHT;
        }
        let block = TextBlock::new(
            vec![TextLine::new(vec![TextSpan::plain(text)])],
            Size::new(size.0, size.1),
            size.1,
        );
        let mut run = TextRun::new(position, font, block);
        run.anchor = style.anchor;
        run.baseline = style.baseline;
        run.fill = Some(Color::new(style.fill));
        run
    }

    /// `.railroad-line`: a stroked, unfilled path.
    fn line_shape(&self, path: Path) -> PathShape {
        PathShape::new(
            path,
            PathStyle {
                fill: None,
                stroke: Some(Color::new(self.options.line_color.clone())),
                stroke_width: self.options.stroke_width,
                ..PathStyle::default()
            },
        )
    }

    /// A filled, stroked rectangle.
    fn boxed(&self, rect: Rect, radius: f64, fill: &str, stroke: &str) -> RectShape {
        RectShape::new(
            rect,
            radius,
            PathStyle {
                fill: Some(Color::new(fill)),
                stroke: Some(Color::new(stroke)),
                stroke_width: self.options.stroke_width,
                ..PathStyle::default()
            },
        )
    }

    /// `renderTerminal`: a rounded rectangle around the literal.
    fn render_terminal(&mut self, value: &str) -> RenderResult {
        let text_dimensions = self.measure_text(value);
        let width = text_dimensions.0 + self.options.padding * 2.0;
        let height = text_dimensions.1 + self.options.padding * 2.0;

        let mut group = Group::new().with_class("railroad-terminal");
        group.push(self.boxed(
            Rect::new(0.0, 0.0, width, height),
            TERMINAL_CORNER_RADIUS,
            &self.options.terminal_fill,
            &self.options.terminal_stroke,
        ));
        group.push(self.text_run(
            value,
            Point::new(width / 2.0, height / 2.0),
            LabelStyle::boxed(&self.options.terminal_text_color),
            text_dimensions,
        ));

        RenderResult {
            group,
            dimensions: Dimensions {
                width,
                height,
                up: height / 2.0,
                down: height / 2.0,
            },
        }
    }

    /// `renderNonTerminal`: a square rectangle around the referenced rule name.
    fn render_non_terminal(&mut self, name: &str) -> RenderResult {
        let text_dimensions = self.measure_text(name);
        let width = text_dimensions.0 + self.options.padding * 2.0;
        let height = text_dimensions.1 + self.options.padding * 2.0;

        let mut group = Group::new().with_class("railroad-nonterminal");
        group.push(self.boxed(
            Rect::new(0.0, 0.0, width, height),
            0.0,
            &self.options.non_terminal_fill,
            &self.options.non_terminal_stroke,
        ));
        group.push(self.text_run(
            name,
            Point::new(width / 2.0, height / 2.0),
            LabelStyle::boxed(&self.options.non_terminal_text_color),
            text_dimensions,
        ));

        RenderResult {
            group,
            dimensions: Dimensions {
                width,
                height,
                up: height / 2.0,
                down: height / 2.0,
            },
        }
    }

    /// `renderSpecial`: a dashed rectangle around `? text ?`.
    fn render_special(&mut self, text: &str) -> RenderResult {
        let label = format!("? {text} ?");
        let text_dimensions = self.measure_text(&label);
        let width = text_dimensions.0 + self.options.padding * 2.0;
        let height = text_dimensions.1 + self.options.padding * 2.0;

        let mut group = Group::new().with_class("railroad-special");
        let mut rect = self.boxed(
            Rect::new(0.0, 0.0, width, height),
            0.0,
            &self.options.special_fill,
            &self.options.special_stroke,
        );
        rect.style = rect.style.with_dash([5.0, 3.0]);
        group.push(rect);
        group.push(self.text_run(
            &label,
            Point::new(width / 2.0, height / 2.0),
            LabelStyle::boxed(&self.options.non_terminal_text_color),
            text_dimensions,
        ));

        RenderResult {
            group,
            dimensions: Dimensions {
                width,
                height,
                up: height / 2.0,
                down: height / 2.0,
            },
        }
    }

    /// `renderSequence`: the elements left to right, joined by short segments.
    fn render_sequence(&mut self, elements: &[Node]) -> RenderResult {
        let rendered: Vec<RenderResult> = elements
            .iter()
            .map(|element| self.render_expression(element))
            .collect();

        let mut total_width = 0.0;
        let mut max_up: f64 = 0.0;
        let mut max_down: f64 = 0.0;
        for result in &rendered {
            total_width += result.dimensions.width;
            max_up = max_up.max(result.dimensions.up);
            max_down = max_down.max(result.dimensions.down);
        }
        if !rendered.is_empty() {
            total_width += (rendered.len() - 1) as f64 * self.options.horizontal_separation;
        }

        let mut group = Group::new().with_class("railroad-sequence");
        let mut x = 0.0;
        let count = rendered.len();
        for (index, result) in rendered.into_iter().enumerate() {
            let y = max_up - result.dimensions.up;
            let width = result.dimensions.width;
            group.push(place(result.group, x, y));

            if index < count - 1 {
                let mut path = Path::new();
                path.move_to(Point::new(x + width, max_up))
                    .line_to(Point::new(
                        x + width + self.options.horizontal_separation,
                        max_up,
                    ));
                group.push(self.line_shape(path));
            }

            x += width + self.options.horizontal_separation;
        }

        RenderResult {
            group,
            dimensions: Dimensions {
                width: total_width,
                height: max_up + max_down,
                up: max_up,
                down: max_down,
            },
        }
    }

    /// `renderChoice`: the alternatives stacked vertically, joined by two arc columns.
    fn render_choice(&mut self, alternatives: &[Node]) -> RenderResult {
        let rendered: Vec<RenderResult> = alternatives
            .iter()
            .map(|alternative| self.render_expression(alternative))
            .collect();

        let mut max_width: f64 = 0.0;
        let mut total_height = 0.0;
        for result in &rendered {
            max_width = max_width.max(result.dimensions.width);
            total_height += result.dimensions.height;
        }
        if !rendered.is_empty() {
            total_height += (rendered.len() - 1) as f64 * self.options.vertical_separation;
        }

        let arc_radius = self.options.arc_radius;
        let arc_width = arc_radius * 4.0;
        let total_width = max_width + arc_width;

        let mut group = Group::new().with_class("railroad-choice");
        let mut y = 0.0;
        let center_y = total_height / 2.0;

        for result in rendered {
            let elem_y = y;
            let elem_center_y = elem_y + result.dimensions.up;
            let elem_x = arc_radius * 2.0 + (max_width - result.dimensions.width) / 2.0;
            let width = result.dimensions.width;
            let height = result.dimensions.height;
            let right_start = elem_x + width;
            let right_lane_x = total_width - arc_radius * 2.0;
            let is_below_center = elem_center_y > center_y;

            group.push(place(result.group, elem_x, elem_y));

            // Left arc from the centre to this alternative.
            let mut left_path = Path::new();
            if elem_center_y == center_y {
                left_path
                    .move_to(Point::new(0.0, center_y))
                    .line_to(Point::new(elem_x, elem_center_y));
            } else {
                left_path
                    .move_to(Point::new(0.0, center_y))
                    .arc_to(
                        arc_radius,
                        arc_radius,
                        0.0,
                        false,
                        is_below_center,
                        Point::new(
                            arc_radius,
                            center_y
                                + if is_below_center {
                                    arc_radius
                                } else {
                                    -arc_radius
                                },
                        ),
                    )
                    .line_to(Point::new(
                        arc_radius,
                        elem_center_y
                            - if is_below_center {
                                arc_radius
                            } else {
                                -arc_radius
                            },
                    ))
                    .arc_to(
                        arc_radius,
                        arc_radius,
                        0.0,
                        false,
                        !is_below_center,
                        Point::new(arc_radius * 2.0, elem_center_y),
                    )
                    .line_to(Point::new(elem_x, elem_center_y));
            }
            group.push(self.line_shape(left_path));

            // Right arc from this alternative back to the centre.
            let mut right_path = Path::new();
            if elem_center_y == center_y {
                right_path
                    .move_to(Point::new(right_start, elem_center_y))
                    .line_to(Point::new(total_width, center_y));
            } else {
                right_path
                    .move_to(Point::new(right_start, elem_center_y))
                    .line_to(Point::new(right_lane_x, elem_center_y))
                    .arc_to(
                        arc_radius,
                        arc_radius,
                        0.0,
                        false,
                        !is_below_center,
                        Point::new(
                            total_width - arc_radius,
                            elem_center_y
                                + if is_below_center {
                                    -arc_radius
                                } else {
                                    arc_radius
                                },
                        ),
                    )
                    .line_to(Point::new(
                        total_width - arc_radius,
                        center_y
                            + if is_below_center {
                                arc_radius
                            } else {
                                -arc_radius
                            },
                    ))
                    .arc_to(
                        arc_radius,
                        arc_radius,
                        0.0,
                        false,
                        is_below_center,
                        Point::new(total_width, center_y),
                    );
            }
            group.push(self.line_shape(right_path));

            y += height + self.options.vertical_separation;
        }

        RenderResult {
            group,
            dimensions: Dimensions {
                width: total_width,
                height: total_height,
                up: center_y,
                down: total_height - center_y,
            },
        }
    }

    /// `renderOptional`: the element with an upper bypass.
    fn render_optional(&mut self, element: &Node) -> RenderResult {
        let inner = self.render_expression(element);
        let arc_radius = self.options.arc_radius;
        let arc_height = arc_radius * 2.0;
        let inner_width = inner.dimensions.width;
        let inner_height = inner.dimensions.height;
        let inner_up = inner.dimensions.up;
        let total_width = inner_width + arc_radius * 4.0;
        let total_height = inner_height + arc_height;

        let mut group = Group::new().with_class("railroad-optional");

        let elem_x = arc_radius * 2.0;
        let elem_y = arc_height;
        group.push(place(inner.group, elem_x, elem_y));

        let center_y = elem_y + inner_up;

        // Lower path (through the element).
        let mut lower_path = Path::new();
        lower_path
            .move_to(Point::new(0.0, center_y))
            .line_to(Point::new(arc_radius * 2.0, center_y));
        group.push(self.line_shape(lower_path));

        let mut lower_path_2 = Path::new();
        lower_path_2
            .move_to(Point::new(elem_x + inner_width, center_y))
            .line_to(Point::new(total_width, center_y));
        group.push(self.line_shape(lower_path_2));

        // Upper bypass path.
        let mut bypass = Path::new();
        bypass
            .move_to(Point::new(0.0, center_y))
            .arc_to(
                arc_radius,
                arc_radius,
                0.0,
                false,
                false,
                Point::new(arc_radius, center_y - arc_radius),
            )
            .line_to(Point::new(arc_radius, arc_radius))
            .arc_to(
                arc_radius,
                arc_radius,
                0.0,
                false,
                true,
                Point::new(arc_radius * 2.0, 0.0),
            )
            .line_to(Point::new(total_width - arc_radius * 2.0, 0.0))
            .arc_to(
                arc_radius,
                arc_radius,
                0.0,
                false,
                true,
                Point::new(total_width - arc_radius, arc_radius),
            )
            .line_to(Point::new(total_width - arc_radius, center_y - arc_radius))
            .arc_to(
                arc_radius,
                arc_radius,
                0.0,
                false,
                false,
                Point::new(total_width, center_y),
            );
        group.push(self.line_shape(bypass));

        RenderResult {
            group,
            dimensions: Dimensions {
                width: total_width,
                height: total_height,
                up: center_y,
                down: total_height - center_y,
            },
        }
    }

    /// `renderRepetition`: the element with a lower loop, and an upper bypass for `*`.
    fn render_repetition(&mut self, element: &Node, min: usize) -> RenderResult {
        let inner = self.render_expression(element);
        let arc_radius = self.options.arc_radius;
        let arc_height = arc_radius * 2.0;
        let inner_width = inner.dimensions.width;
        let inner_height = inner.dimensions.height;
        let inner_up = inner.dimensions.up;
        let total_width = inner_width + arc_radius * 4.0;

        // For `*`, add the bypass on top; for `+`, no bypass.
        let has_bypass = min == 0;
        let total_height = inner_height + arc_height + if has_bypass { arc_height } else { 0.0 };

        let mut group = Group::new().with_class("railroad-repetition");

        let elem_x = arc_radius * 2.0;
        let elem_y = if has_bypass { arc_height } else { 0.0 };
        group.push(place(inner.group, elem_x, elem_y));

        let center_y = elem_y + inner_up;

        // Forward path.
        let mut forward = Path::new();
        forward
            .move_to(Point::new(0.0, center_y))
            .line_to(Point::new(arc_radius * 2.0, center_y));
        group.push(self.line_shape(forward));

        let mut forward_2 = Path::new();
        forward_2
            .move_to(Point::new(elem_x + inner_width, center_y))
            .line_to(Point::new(total_width, center_y));
        group.push(self.line_shape(forward_2));

        // Loop back path (below).
        let loop_y = elem_y + inner_height + arc_radius;
        let mut loop_path = Path::new();
        loop_path
            .move_to(Point::new(elem_x + inner_width, center_y))
            .arc_to(
                arc_radius,
                arc_radius,
                0.0,
                false,
                true,
                Point::new(elem_x + inner_width + arc_radius, center_y + arc_radius),
            )
            .line_to(Point::new(elem_x + inner_width + arc_radius, loop_y))
            .arc_to(
                arc_radius,
                arc_radius,
                0.0,
                false,
                true,
                Point::new(elem_x + inner_width, loop_y + arc_radius),
            )
            .line_to(Point::new(arc_radius * 2.0, loop_y + arc_radius))
            .arc_to(
                arc_radius,
                arc_radius,
                0.0,
                false,
                true,
                Point::new(arc_radius, loop_y),
            )
            .line_to(Point::new(arc_radius, center_y + arc_radius))
            .arc_to(
                arc_radius,
                arc_radius,
                0.0,
                false,
                true,
                Point::new(arc_radius * 2.0, center_y),
            );
        group.push(self.line_shape(loop_path));

        // Optional bypass path (for `*`).
        if has_bypass {
            let mut bypass = Path::new();
            bypass
                .move_to(Point::new(0.0, center_y))
                .arc_to(
                    arc_radius,
                    arc_radius,
                    0.0,
                    false,
                    false,
                    Point::new(arc_radius, center_y - arc_radius),
                )
                .line_to(Point::new(arc_radius, arc_radius))
                .arc_to(
                    arc_radius,
                    arc_radius,
                    0.0,
                    false,
                    true,
                    Point::new(arc_radius * 2.0, 0.0),
                )
                .line_to(Point::new(total_width - arc_radius * 2.0, 0.0))
                .arc_to(
                    arc_radius,
                    arc_radius,
                    0.0,
                    false,
                    true,
                    Point::new(total_width - arc_radius, arc_radius),
                )
                .line_to(Point::new(total_width - arc_radius, center_y - arc_radius))
                .arc_to(
                    arc_radius,
                    arc_radius,
                    0.0,
                    false,
                    false,
                    Point::new(total_width, center_y),
                );
            group.push(self.line_shape(bypass));
        }

        RenderResult {
            group,
            dimensions: Dimensions {
                width: total_width,
                height: total_height,
                up: center_y,
                down: total_height - center_y,
            },
        }
    }

    /// `renderExpression`: the seven node types.
    fn render_expression(&mut self, node: &Node) -> RenderResult {
        match node {
            Node::Terminal { value } => self.render_terminal(value),
            Node::NonTerminal { name } => self.render_non_terminal(name),
            Node::Sequence { elements } => self.render_sequence(elements),
            Node::Choice { alternatives } => self.render_choice(alternatives),
            Node::Optional { element } => self.render_optional(element),
            Node::Repetition { element, min, .. } => self.render_repetition(element, *min),
            Node::Special { text } => self.render_special(text),
        }
    }

    /// `renderRule`: one rule's name, definition, markers, and connecting segments.
    fn render_rule(&mut self, rule: &Rule, y: f64) -> (f64, f64, Group) {
        let mut group = Group::translated(0.0, y).with_class("railroad-rule");

        let rule_name = format!("{} =", rule.name);
        let name_width = self.measure_text(&rule_name).0 + RULE_NAME_GAP;
        let definition_x = name_width + RULE_NAME_GAP;

        // The definition is rendered first, and its group is translated into place.
        let result = self.render_expression(&rule.definition);
        let baseline_y = MIN_BASELINE.max(result.dimensions.up);
        let definition_y = baseline_y - result.dimensions.up;
        let definition_width = result.dimensions.width;
        let definition_height = result.dimensions.height;
        let mut definition_group = Group::translated(definition_x, definition_y);
        definition_group.push(result.group);
        group.push(definition_group);

        let name_dimensions = self.measure_text(&rule_name);
        let mut name_group = Group::new().with_class("railroad-rule-name-group");
        name_group.push(self.text_run(
            &rule_name,
            Point::new(0.0, baseline_y),
            LabelStyle::rule_name(&self.options.rule_name_color),
            name_dimensions,
        ));
        group.push(name_group);

        // Start marker.
        let mut start_marker = Group::new().with_class("railroad-start");
        start_marker.push(EllipseShape::new(
            Point::new(name_width, baseline_y),
            self.options.marker_radius,
            self.options.marker_radius,
            PathStyle::filled(Color::new(self.options.marker_fill.clone())),
        ));
        group.push(start_marker);

        // End marker.
        let mut end_marker = Group::new().with_class("railroad-end");
        end_marker.push(EllipseShape::new(
            Point::new(
                definition_x + definition_width + DEFINITION_END_GAP,
                baseline_y,
            ),
            self.options.marker_radius,
            self.options.marker_radius,
            PathStyle::filled(Color::new(self.options.marker_fill.clone())),
        ));
        group.push(end_marker);

        // Line from the start marker to the definition.
        let mut start_line = Path::new();
        start_line
            .move_to(Point::new(
                name_width + self.options.marker_radius,
                baseline_y,
            ))
            .line_to(Point::new(definition_x, baseline_y));
        group.push(self.line_shape(start_line));

        // Line from the definition to the end marker.
        let mut end_line = Path::new();
        end_line
            .move_to(Point::new(definition_x + definition_width, baseline_y))
            .line_to(Point::new(
                definition_x + definition_width + DEFINITION_END_GAP - self.options.marker_radius,
                baseline_y,
            ));
        group.push(self.line_shape(end_line));

        let height =
            MIN_RULE_HEIGHT.max(definition_y + definition_height + self.options.padding * 2.0);
        let width =
            definition_x + definition_width + DEFINITION_END_GAP + self.options.marker_radius;
        (height, width, group)
    }

    /// `renderDiagram`: every rule top to bottom, and the viewport around them.
    fn render_diagram(&mut self, rules: &[Rule]) -> Scene {
        if rules.is_empty() {
            return Scene::new(Rect::new(0.0, 0.0, EMPTY_VIEW_WIDTH, EMPTY_VIEW_HEIGHT));
        }

        let mut y = self.options.padding;
        let mut max_width: f64 = 0.0;
        let mut groups = Vec::with_capacity(rules.len());
        for rule in rules {
            let (height, width, group) = self.render_rule(rule, y);
            y += height + self.options.vertical_separation;
            max_width = max_width.max(width);
            groups.push(group);
        }

        let width = max_width + self.options.padding * 2.0;
        let height = y + self.options.padding;
        let mut scene = Scene::new(Rect::new(0.0, 0.0, width, height));
        for group in groups {
            scene.push(group);
        }
        scene
    }
}

/// Moves a rendered group into place, SVG's `transform="translate(x, y)"`.
fn place(mut group: Group, x: f64, y: f64) -> Group {
    group.transform = Transform::translate(x, y);
    group
}

/// Renders a railroad body (the text [`super::parse`] accepts) to a [`Scene`], with the
/// crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, ParseError> {
    draw(body, config, &crate::measure::FallbackMeasure)
}

/// Renders a railroad body to a [`Scene`], measuring text with `measure`.
pub fn draw(body: &str, config: &Config, measure: &dyn TextMeasure) -> Result<Scene, ParseError> {
    let ast = super::parse(body)?;
    Ok(draw_ast(&ast, config, measure))
}

/// Renders an already parsed railroad database.
pub fn draw_ast(ast: &Ast, config: &Config, measure: &dyn TextMeasure) -> Scene {
    let options = StyleOptions::from_config(config);
    let mut renderer = RailroadRenderer::new(options, measure);
    renderer.render_diagram(&ast.rules)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Item;
    use serde_json::json;

    const TOLERANCE: f64 = 0.01;

    fn fixtures() -> Vec<(String, Value)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/diagrams/railroad/tests/render");
        let mut fixtures: Vec<(String, Value)> = std::fs::read_dir(&dir)
            .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
            .map(|entry| entry.expect("dir entry").path())
            .filter(|path| {
                path.extension().and_then(|extension| extension.to_str()) == Some("json")
            })
            .map(|path| {
                let text = std::fs::read_to_string(&path).expect("read fixture");
                let value: Value = serde_json::from_str(&text).expect("parse fixture");
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

    fn close_value(actual: &Value, expected: &Value, what: &str, file: &str) {
        match (actual, expected) {
            (Value::Number(actual), Value::Number(expected)) => close(
                actual.as_f64().expect("number"),
                expected.as_f64().expect("number"),
                what,
                file,
            ),
            (Value::Array(actual), Value::Array(expected)) => {
                assert_eq!(actual.len(), expected.len(), "{file}: {what}: length");
                for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                    close_value(actual, expected, &format!("{what}[{index}]"), file);
                }
            }
            (Value::Object(actual), Value::Object(expected)) => {
                for (key, expected) in expected {
                    // `class` names the CSS selector, and `d` is the path's string form; the
                    // scene carries paints and commands, which are compared field by field.
                    if key == "class" || key == "d" {
                        continue;
                    }
                    close_value(
                        actual.get(key).unwrap_or(&Value::Null),
                        expected,
                        &format!("{what}.{key}"),
                        file,
                    );
                }
            }
            _ => assert_eq!(actual, expected, "{file}: {what}"),
        }
    }

    fn paint_value(style: &PathStyle) -> Value {
        json!({
            "fill": style
                .fill
                .as_ref()
                .map(|color| color.0.clone())
                .unwrap_or_else(|| "none".to_string()),
            "stroke": style.stroke.as_ref().map(|color| color.0.clone()),
            "strokeWidth": style.stroke_width,
            "dash": style.dash,
        })
    }

    fn translated_command(command: &crate::scene::PathCommand, transform: Transform) -> Value {
        use crate::scene::PathCommand::*;
        match *command {
            MoveTo { x, y } => {
                let point = transform.apply(Point::new(x, y));
                json!({ "op": "moveTo", "x": point.x, "y": point.y })
            }
            LineTo { x, y } => {
                let point = transform.apply(Point::new(x, y));
                json!({ "op": "lineTo", "x": point.x, "y": point.y })
            }
            CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => {
                let control1 = transform.apply(Point::new(c1x, c1y));
                let control2 = transform.apply(Point::new(c2x, c2y));
                let point = transform.apply(Point::new(x, y));
                json!({
                    "op": "cubicTo",
                    "c1x": control1.x,
                    "c1y": control1.y,
                    "c2x": control2.x,
                    "c2y": control2.y,
                    "x": point.x,
                    "y": point.y,
                })
            }
            QuadTo { cx, cy, x, y } => {
                let control = transform.apply(Point::new(cx, cy));
                let point = transform.apply(Point::new(x, y));
                json!({
                    "op": "quadTo",
                    "cx": control.x,
                    "cy": control.y,
                    "x": point.x,
                    "y": point.y,
                })
            }
            ArcTo {
                rx,
                ry,
                rotation,
                large_arc,
                sweep,
                x,
                y,
            } => {
                let point = transform.apply(Point::new(x, y));
                json!({
                    "op": "arcTo",
                    "rx": rx,
                    "ry": ry,
                    "rotation": rotation,
                    "large_arc": large_arc,
                    "sweep": sweep,
                    "x": point.x,
                    "y": point.y,
                })
            }
            Close => json!({ "op": "close" }),
        }
    }

    /// Flattens the scene's groups into the absolute shapes the fixture format records.
    fn flatten(items: &[Item], transform: Transform, out: &mut Vec<Value>) {
        for item in items {
            match item {
                Item::Group(group) => {
                    flatten(&group.items, transform.then(group.transform), out);
                }
                Item::Rect(rect) => {
                    let top_left = transform.apply(Point::new(rect.rect.x, rect.rect.y));
                    let bottom_right =
                        transform.apply(Point::new(rect.rect.right(), rect.rect.bottom()));
                    out.push(json!({
                        "kind": "rect",
                        "x": top_left.x,
                        "y": top_left.y,
                        "width": bottom_right.x - top_left.x,
                        "height": bottom_right.y - top_left.y,
                        "rx": rect.rx,
                        "ry": rect.ry,
                        "paint": paint_value(&rect.style),
                    }));
                }
                Item::Ellipse(ellipse) => {
                    let center = transform.apply(ellipse.center);
                    out.push(json!({
                        "kind": "circle",
                        "cx": center.x,
                        "cy": center.y,
                        "r": ellipse.radius_x,
                        "paint": paint_value(&ellipse.style),
                    }));
                }
                Item::Path(shape) => {
                    let commands: Vec<Value> = shape
                        .path
                        .commands
                        .iter()
                        .map(|command| translated_command(command, transform))
                        .collect();
                    out.push(json!({
                        "kind": "path",
                        "commands": commands,
                        "paint": paint_value(&shape.style),
                    }));
                }
                Item::Text(text) => {
                    let position = transform.apply(text.position);
                    out.push(json!({
                        "kind": "text",
                        "text": text.lines.iter().map(|line| line.text()).collect::<Vec<_>>().join("\n"),
                        "x": position.x,
                        "y": position.y,
                        "anchor": serde_json::to_value(text.anchor).expect("anchor serializes"),
                        "baseline": serde_json::to_value(text.baseline).expect("baseline serializes"),
                        "fontFamily": text.font.family,
                        "fontSize": text.font.size,
                        "bold": text.font.weight >= BOLD_WEIGHT,
                        "italic": text.font.italic,
                        "paint": json!({
                            "fill": text
                                .fill
                                .as_ref()
                                .map(|color| color.0.clone())
                                .unwrap_or_else(|| "black".to_string()),
                            "stroke": null,
                            "strokeWidth": 1.0,
                            "dash": [],
                        }),
                    }));
                }
                Item::Image(image) => panic!("railroad draws no image: {image:?}"),
            }
        }
    }

    #[test]
    fn renders_recorded_railroad_fixtures() {
        let fixtures = fixtures();
        assert!(
            fixtures.len() >= 8,
            "expected at least 8 fixtures, found {}",
            fixtures.len()
        );

        for (file, fixture) in fixtures {
            let source = fixture["source"].as_str().expect("fixture source");
            let preprocessed = crate::preprocess::preprocess(source);
            let config = Config::from_source_config(&preprocessed.config)
                .unwrap_or_else(|error| panic!("{file}: config: {error}"));
            let scene = render(&preprocessed.body, &config)
                .unwrap_or_else(|error| panic!("{file}: render: {error}"));
            let expected = &fixture["expected"];

            let expected_view_box = &expected["viewBox"];
            close(
                scene.view_box.x,
                expected_view_box["x"].as_f64().expect("viewBox x"),
                "viewBox x",
                &file,
            );
            close(
                scene.view_box.y,
                expected_view_box["y"].as_f64().expect("viewBox y"),
                "viewBox y",
                &file,
            );
            close(
                scene.view_box.width,
                expected_view_box["width"].as_f64().expect("viewBox width"),
                "viewBox width",
                &file,
            );
            close(
                scene.view_box.height,
                expected_view_box["height"]
                    .as_f64()
                    .expect("viewBox height"),
                "viewBox height",
                &file,
            );

            let mut actual = Vec::new();
            flatten(&scene.items, Transform::IDENTITY, &mut actual);

            let expected_shapes = expected["shapes"].as_array().expect("shapes");
            assert_eq!(actual.len(), expected_shapes.len(), "{file}: shape count");
            for (index, (actual, expected)) in actual.iter().zip(expected_shapes).enumerate() {
                close_value(actual, expected, &format!("shape {index}"), &file);
            }
        }
    }

    #[test]
    fn a_diagram_without_rules_uses_the_empty_viewport() {
        let scene = render("railroad-beta\n", &Config::default()).expect("render");
        assert_eq!(scene.view_box, Rect::new(0.0, 0.0, 200.0, 100.0));
        assert!(scene.items.is_empty());
    }

    #[test]
    fn render_diagram_dispatches_railroad() {
        let scene = crate::render_diagram(
            "railroad-beta\nStart = terminal(\"begin\");\n",
            &Config::default(),
        )
        .expect("railroad renders");
        assert!(!scene.items.is_empty());
    }

    #[test]
    fn a_rule_group_holds_the_definition_name_and_markers() {
        let scene = render(
            "railroad-beta\nStart = terminal(\"begin\");\n",
            &Config::default(),
        )
        .expect("render");
        let rule = match &scene.items[0] {
            Item::Group(group) => group,
            _ => panic!("rule is not a group"),
        };
        assert_eq!(rule.class.as_deref(), Some("railroad-rule"));

        let classes: Vec<Option<&str>> = rule
            .items
            .iter()
            .map(|item| match item {
                Item::Group(group) => group.class.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(
            classes,
            vec![
                None,
                Some("railroad-rule-name-group"),
                Some("railroad-start"),
                Some("railroad-end"),
                None,
                None,
            ]
        );
    }

    #[test]
    fn terminal_text_uses_the_theme_color_and_centres_on_the_baseline() {
        let config = Config::default();
        let scene =
            render("railroad-beta\nStart = terminal(\"begin\");\n", &config).expect("render");
        let rule = match &scene.items[0] {
            Item::Group(group) => group,
            _ => panic!("rule is not a group"),
        };
        let definition = match &rule.items[0] {
            Item::Group(group) => group,
            _ => panic!("definition is not a group"),
        };
        let terminal = match &definition.items[0] {
            Item::Group(group) => group,
            _ => panic!("terminal is not a group"),
        };
        let text = match &terminal.items[1] {
            Item::Text(text) => text,
            _ => panic!("terminal label is not text"),
        };
        assert_eq!(text.anchor, TextAnchor::Middle);
        assert_eq!(text.baseline, TextBaseline::Middle);
        assert_eq!(
            text.fill.as_ref().map(|color| color.0.clone()).as_deref(),
            config
                .theme_variables
                .get("secondaryTextColor")
                .and_then(Value::as_str)
        );
    }

    #[test]
    fn the_railroad_config_overrides_the_theme_layout_and_colors() {
        let mut config = Config::default();
        config.railroad.padding = 4.0;
        config.railroad.arc_radius = 3.0;
        config.railroad.font_family = Some("monospace".to_string());
        config.railroad.terminal_fill = Some("#123456".to_string());
        let scene = render("railroad-beta\nStart = terminal(\"a\");\n", &config).expect("render");
        let rule = match &scene.items[0] {
            Item::Group(group) => group,
            _ => panic!("rule is not a group"),
        };
        let definition = match &rule.items[0] {
            Item::Group(group) => group,
            _ => panic!("definition is not a group"),
        };
        let terminal = match &definition.items[0] {
            Item::Group(group) => group,
            _ => panic!("terminal is not a group"),
        };
        let rect = match &terminal.items[0] {
            Item::Rect(rect) => rect,
            _ => panic!("terminal box is not a rect"),
        };
        assert_eq!(rect.style.fill, Some(Color::new("#123456")));
        // One character at the fallback monospace advance (0.6 em) plus 4 units of padding.
        close(
            rect.rect.width,
            0.6 * 16.0 + 8.0,
            "terminal width",
            "config",
        );
    }

    #[test]
    fn a_special_box_is_dashed_and_wrapped_in_question_marks() {
        let scene = render(
            "railroad-beta\nLetter = special(\"letter\");\n",
            &Config::default(),
        )
        .expect("render");
        let rule = match &scene.items[0] {
            Item::Group(group) => group,
            _ => panic!("rule is not a group"),
        };
        let definition = match &rule.items[0] {
            Item::Group(group) => group,
            _ => panic!("definition is not a group"),
        };
        let special = match &definition.items[0] {
            Item::Group(group) => group,
            _ => panic!("special is not a group"),
        };
        assert_eq!(special.class.as_deref(), Some("railroad-special"));
        let rect = match &special.items[0] {
            Item::Rect(rect) => rect,
            _ => panic!("special box is not a rect"),
        };
        assert_eq!(rect.style.dash, vec![5.0, 3.0]);
        let text = match &special.items[1] {
            Item::Text(text) => text,
            _ => panic!("special label is not text"),
        };
        assert_eq!(text.lines[0].text(), "? letter ?");
    }

    #[test]
    fn a_repetition_keeps_up_to_two_tiers_above_and_below_the_baseline() {
        let zero = render(
            "railroad-beta\nList = zeroOrMore(nonterminal(\"Item\"));\n",
            &Config::default(),
        )
        .expect("render");
        let one = render(
            "railroad-beta\nList = oneOrMore(nonterminal(\"Item\"));\n",
            &Config::default(),
        )
        .expect("render");
        assert!(zero.view_box.height > one.view_box.height);
    }

    #[test]
    fn rejects_a_body_without_the_railroad_header() {
        let error = render("Start = terminal(\"a\");\n", &Config::default()).unwrap_err();
        assert!(
            error.message.contains("railroad"),
            "unexpected message: {}",
            error.message
        );
    }
}
