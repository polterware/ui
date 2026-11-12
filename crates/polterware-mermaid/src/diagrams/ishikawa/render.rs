//! Source-to-[`Scene`] rendering for ishikawa (fishbone) diagrams: the Rust side of
//! `ishikawaRenderer.ts` `draw`.
//!
//! An ishikawa diagram does not go through the unified layout pipeline: `draw` appends plain
//! SVG (a spine `<line>`, one `<g class="ishikawa-head-group">` with the fish head, one
//! `<g class="ishikawa-pair">` per pair of causes, and `<line>`/`<rect>`/`<text>` leaves inside
//! them) and fits the viewport with its own `applyPaddedViewBox`. This module ports that:
//! [`draw`] parses, [`draw_ast`] renders an already parsed database, and [`render`] uses the
//! crate's monospace fallback measurement.
//!
//! Look: `diagrams/ishikawa/ishikawaStyles.ts` and the attributes `ishikawaRenderer.ts` writes.
//! The spine and branch lines are `lineColor` at 2px, sub-branches 1px; the head and the cause
//! label boxes are `mainBkg` filled and `lineColor` stroked at 2px; every label is `textColor`;
//! the arrow marker is a filled `lineColor` triangle. The head label is 14px (the stylesheet
//! hardcodes it) while every other label takes the theme's `fontSize`; the line advance inside
//! a wrapped label is `drawMultilineText`'s `fontSize * 1.05`, and the head wraps at
//! `max(6, floor(110 / (fontSize * 0.6)))` characters while a sub-cause wraps at 15.
//!
//! Behavior: the renderer itself is the model. gpui-kit has no ishikawa/fishbone diagram, and
//! this renderer uses no layout engine, so there is no second behavior reference; every step
//! follows `draw`, `drawHead`, `drawBranch`, `drawCauseLabel`, `flattenTree` and `sideStats`
//! call by call.
//!
//! Where the port stops short of upstream, the difference is named here:
//!
//! * `look: 'handDrawn'` draws the same geometry with rough.js hatched strokes and a
//!   hand-drawn head and label boxes. This crate has no rough.js port, so the `handDrawn` look
//!   draws the `classic` one; the geometry is identical because upstream's rough branch only
//!   swaps the paint of the same coordinates.
//! * CSS classes (`ishikawa-spine`, `ishikawa-label cause`, ...) matter only to the stylesheet,
//!   and a [`Scene`] carries paints rather than selectors, so `draw` resolves their declarations
//!   into the shapes. The class names survive on the groups ([`Group::class`]) where they
//!   structure the drawing (`.ishikawa`, `.ishikawa-head-group`, `.ishikawa-label-group`,
//!   `.ishikawa-sub-group`, `.ishikawa-pair`).
//! * The stylesheet's `text-anchor` and `dominant-baseline` rules override the attributes
//!   `drawMultilineText` writes, and a browser applies the stylesheet: the head label is
//!   written with `text-anchor="start"` but `.ishikawa-head-label` centres it, and the cause
//!   and sub-cause labels are centred or end-anchored with the baseline `.cause`/`.align`/
//!   `.up`/`.down` name. The scene stores the anchor and baseline a browser draws with, at the
//!   position upstream's own `getBBox` arithmetic produces, so the painted glyphs land where
//!   the browser puts them.
//! * `applyPaddedViewBox` calls `getBBox()` on the whole SVG; here the same union is taken
//!   over the painted items (stroke and markers excluded, as SVG's `getBBox` excludes them),
//!   and `useMaxWidth` only sizes the `<svg>` element, so the scene carries the fitted view box
//!   and the painter sizes the page.
//! * The marker id `ishikawa-arrow-${id}` embeds the diagram id; the scene uses the stable
//!   `ishikawa-arrow`, one per diagram.
//!
//! Text is measured in the `monospace` family, the rule the fixture harness renders with and
//! that [`FallbackMeasure`](crate::measure::FallbackMeasure) reproduces; the theme's family
//! would measure differently from the harness's browser font. A label's ink box is
//! `1.15 * fontSize` tall plus the line advance for every row after the first, exactly the box
//! the harness's `getBBox` reports.

use std::collections::HashMap;

use crate::config::Config;
use crate::error::ParseError;
use crate::measure::{Font, TextMeasure};
use crate::render::shapes::label_image_utils::parse_font_size;
use crate::scene::{
    Color, Group, Item, MarkerDef, MarkerOrient, MarkerRef, Path, PathShape, PathStyle, Point,
    Rect, RectShape, Scene, Size, TextAnchor, TextBaseline, TextBlock, TextLine, TextRun, TextSpan,
};

use super::Ast;
use super::ast::IshikawaNode;

/// `FONT_SIZE_DEFAULT`, `draw`'s fallback when `parseFontSize` finds no number.
pub const FONT_SIZE_DEFAULT: f64 = 14.0;
/// `SPINE_BASE_LENGTH`: the spine's base length and the head's baseline offset.
pub const SPINE_BASE_LENGTH: f64 = 250.0;
/// `BONE_STUB`: how far a childless cause's label bone extends.
pub const BONE_STUB: f64 = 30.0;
/// `BONE_BASE`: the label bone's extra length before `BONE_PER_CHILD` per child.
pub const BONE_BASE: f64 = 60.0;
/// `BONE_PER_CHILD`: the label bone's length added for each child.
pub const BONE_PER_CHILD: f64 = 5.0;
/// `ANGLE`: the branch angle in degrees (`82 * Math.PI / 180`).
pub const ANGLE_DEGREES: f64 = 82.0;
/// `.ishikawa-head-label`'s hardcoded `font-size: 14px`.
pub const HEAD_LABEL_FONT_SIZE: f64 = 14.0;
/// `drawHead`'s `maxChars` width budget, `110 / (fontSize * 0.6)`.
pub const HEAD_LABEL_WIDTH_BUDGET: f64 = 110.0;
/// `drawHead`'s `Math.max(60, ...)` head width.
pub const HEAD_MIN_WIDTH: f64 = 60.0;
/// `drawHead`'s `Math.max(40, ...)` head height.
pub const HEAD_MIN_HEIGHT: f64 = 40.0;
/// `drawCauseLabel`'s box padding around the measured text, horizontally.
pub const LABEL_BOX_PADDING_X: f64 = 20.0;
/// `drawCauseLabel`'s box padding around the measured text, vertically.
pub const LABEL_BOX_PADDING_Y: f64 = 2.0;
/// `drawCauseLabel`'s `y + 11 * direction`.
pub const CAUSE_LABEL_OFFSET: f64 = 11.0;
/// `draw`'s `spineX -= 20` before the first pair.
pub const SPINE_INDENT: f64 = 20.0;
/// `flattenTree`'s `wrapText(child.text, 15)`.
pub const WRAP_CHARS: usize = 15;
/// The id the arrow marker is defined under and referenced by.
pub const MARKER_ID: &str = "ishikawa-arrow";
/// `drawMultilineText`'s `lh = fontSize * 1.05`.
pub const LINE_ADVANCE_EM: f64 = 1.05;
/// `drawHead`'s `fontSize * 0.6` monospace advance in the `maxChars` formula.
const HEAD_ADVANCE_EM: f64 = 0.6;

/// Renders an ishikawa body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(
    body: &str,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, IshikawaRenderError> {
    let ast = super::parse(body)?;
    Ok(draw_ast(&ast, config, measure))
}

/// Renders an already parsed ishikawa database.
pub fn draw_ast(ast: &Ast, config: &Config, measure: &dyn TextMeasure) -> Scene {
    let Some(root) = ast.root.as_ref() else {
        // `draw` returns before appending anything when the database has no root, so the SVG
        // holds no drawing and no viewBox.
        return Scene::default();
    };

    let theme = &config.theme_variables;
    let font_size = config.font_size;
    let label_size = theme
        .get("fontSize")
        .and_then(|value| value.as_str())
        .and_then(parse_font_size)
        .unwrap_or(font_size);
    let label_font = Font::new("monospace", label_size);
    let head_font = Font::new("monospace", HEAD_LABEL_FONT_SIZE);
    let line_advance = font_size * LINE_ADVANCE_EM;

    let painter = Painter {
        measure,
        label_font,
        head_font,
        line_advance,
        head_max_chars: head_max_chars(font_size),
        line_color: theme_color(theme, "lineColor", "#333333"),
        main_bkg: theme_color(theme, "mainBkg", "#fff"),
        text_color: theme_color(theme, "textColor", "#333"),
    };

    let causes = &root.children;
    let mut spine_x = 0.0;
    let mut spine_y = SPINE_BASE_LENGTH;
    let mut pairs: Vec<Group> = Vec::new();

    if !causes.is_empty() {
        spine_x -= SPINE_INDENT;

        let upper: Vec<&IshikawaNode> = causes.iter().step_by(2).collect();
        let lower: Vec<&IshikawaNode> = causes.iter().skip(1).step_by(2).collect();
        let upper_stats = side_stats(&upper);
        let lower_stats = side_stats(&lower);
        let descendant_total = upper_stats.total + lower_stats.total;

        let mut upper_len = SPINE_BASE_LENGTH;
        let mut lower_len = SPINE_BASE_LENGTH;
        if descendant_total > 0.0 {
            let pool = SPINE_BASE_LENGTH * 2.0;
            let min_len = SPINE_BASE_LENGTH * 0.3;
            upper_len = min_len.max(pool * (upper_stats.total / descendant_total));
            lower_len = min_len.max(pool * (lower_stats.total / descendant_total));
        }

        let min_spacing = font_size * 2.0;
        upper_len = upper_len.max(upper_stats.max * min_spacing);
        lower_len = lower_len.max(lower_stats.max * min_spacing);

        spine_y = upper_len.max(SPINE_BASE_LENGTH);

        let angle = ANGLE_DEGREES.to_radians();
        let cos_a = angle.cos();
        let sin_a = angle.sin();
        let pair_count = causes.len().div_ceil(2);
        for pair_index in 0..pair_count {
            let mut pair = Group::new().with_class("ishikawa-pair");
            let sides = [
                (causes.get(pair_index * 2), -1.0, upper_len),
                (causes.get(pair_index * 2 + 1), 1.0, lower_len),
            ];
            let mut pair_min_x: Option<f64> = None;
            for (cause, direction, length) in sides {
                let Some(cause) = cause else {
                    continue;
                };
                if let Some(min_x) = painter.branch(
                    &mut pair, cause, spine_x, spine_y, direction, length, cos_a, sin_a,
                ) {
                    pair_min_x = Some(pair_min_x.map_or(min_x, |current| current.min(min_x)));
                }
            }
            if let Some(min_x) = pair_min_x {
                spine_x = min_x;
            }
            pairs.push(pair);
        }
    }

    let mut drawing = Group::new().with_class("ishikawa");
    drawing.push(painter.line_shape(spine_x, spine_y, 0.0, spine_y, 2.0, false));
    drawing.push(painter.head(spine_y, &root.text));
    for pair in pairs {
        drawing.push(pair);
    }

    let mut scene = Scene::default();
    scene.define(painter.marker_def());
    scene.push(drawing);

    let bounds = scene_bounds(&scene.items).unwrap_or(Rect::ZERO);
    scene.bounds = bounds;
    scene.view_box = bounds.expanded(config.ishikawa.diagram_padding);
    scene
}

/// Renders an ishikawa body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, IshikawaRenderError> {
    draw(body, config, &crate::measure::FallbackMeasure)
}

/// What can stop an ishikawa render.
#[derive(Clone, Debug, PartialEq)]
pub enum IshikawaRenderError {
    Parse(ParseError),
}

impl std::fmt::Display for IshikawaRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for IshikawaRenderError {}

impl From<ParseError> for IshikawaRenderError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

/// `sideStats`: how many descendants each cause of one side has, their sum, and the largest.
struct SideStats {
    total: f64,
    max: f64,
}

fn side_stats(nodes: &[&IshikawaNode]) -> SideStats {
    fn count_descendants(node: &IshikawaNode) -> f64 {
        node.children
            .iter()
            .map(|child| 1.0 + count_descendants(child))
            .sum()
    }

    let mut stats = SideStats {
        total: 0.0,
        max: 0.0,
    };
    for node in nodes {
        let descendants = count_descendants(node);
        stats.total += descendants;
        stats.max = stats.max.max(descendants);
    }
    stats
}

/// `drawHead`'s `maxChars`: the head label wraps so the head stays about 110px wide.
fn head_max_chars(font_size: f64) -> usize {
    let budget = HEAD_LABEL_WIDTH_BUDGET / (font_size * HEAD_ADVANCE_EM);
    (budget.floor() as usize).max(6)
}

fn theme_color(
    theme: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    fallback: &str,
) -> Option<Color> {
    Some(Color::new(
        theme
            .get(key)
            .and_then(|value| value.as_str())
            .unwrap_or(fallback),
    ))
}

/// One bone of the flattened cause tree: the line it draws and where its children attach.
#[derive(Clone, Copy)]
struct Bone {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    child_count: usize,
    children_drawn: usize,
}

/// One entry of `flattenTree`: a label, its depth, and the bone it hangs from.
struct LabelEntry {
    text: String,
    depth: usize,
    parent: i32,
    child_count: usize,
}

/// `flattenTree`: flatten the children so their Y positions can be assigned without recursion
/// while drawing. Even depths are placed in pre-order (close to the spine), odd depths in
/// post-order (within the parent's diagonal).
fn flatten_tree(children: &[IshikawaNode], direction: f64) -> (Vec<LabelEntry>, Vec<usize>) {
    fn walk(
        nodes: &[IshikawaNode],
        parent: i32,
        depth: usize,
        direction: f64,
        entries: &mut Vec<LabelEntry>,
        y_order: &mut Vec<usize>,
    ) {
        let ordered: Vec<&IshikawaNode> = if direction < 0.0 {
            nodes.iter().rev().collect()
        } else {
            nodes.iter().collect()
        };
        for child in ordered {
            let index = entries.len();
            entries.push(LabelEntry {
                text: wrap_text(&child.text, WRAP_CHARS),
                depth,
                parent,
                child_count: child.children.len(),
            });
            if depth.is_multiple_of(2) {
                y_order.push(index);
                if !child.children.is_empty() {
                    walk(
                        &child.children,
                        index as i32,
                        depth + 1,
                        direction,
                        entries,
                        y_order,
                    );
                }
            } else {
                if !child.children.is_empty() {
                    walk(
                        &child.children,
                        index as i32,
                        depth + 1,
                        direction,
                        entries,
                        y_order,
                    );
                }
                y_order.push(index);
            }
        }
    }

    let mut entries = Vec::new();
    let mut y_order = Vec::new();
    walk(children, -1, 2, direction, &mut entries, &mut y_order);
    (entries, y_order)
}

/// `draw`'s `wrapText`: greedy wrapping on whitespace at `max_chars` characters, joined with
/// newlines.
pub fn wrap_text(text: &str, max_chars: usize) -> String {
    if text.encode_utf16().count() <= max_chars {
        return text.to_string();
    }
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(last)
                if last.encode_utf16().count() + 1 + word.encode_utf16().count() <= max_chars =>
            {
                last.push(' ');
                last.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines.join("\n")
}

/// `splitLines`: `<br>`/`<br/>`/`<br />` or a newline starts a new row.
fn split_lines(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\n' {
            lines.push(&text[start..index]);
            index += 1;
            start = index;
            continue;
        }
        if bytes[index] == b'<' {
            let rest = &text[index..];
            if let Some(length) = break_tag_length(rest) {
                lines.push(&text[start..index]);
                index += length;
                start = index;
                continue;
            }
        }
        index += 1;
    }
    lines.push(&text[start..]);
    lines
}

/// The length in bytes of a `<br>` tag at the start of `text`, if there is one.
fn break_tag_length(text: &str) -> Option<usize> {
    let mut chars = text.char_indices();
    if chars.next()?.1 != '<' {
        return None;
    }
    // The regex is case-sensitive: an uppercase `<BR>` is text, not a line break.
    if chars.next()?.1 != 'b' {
        return None;
    }
    if chars.next()?.1 != 'r' {
        return None;
    }
    for (index, ch) in chars {
        let end = index + ch.len_utf8();
        match ch {
            '>' => return Some(end),
            '/' if text[end..].starts_with('>') => return Some(end + 1),
            '/' => return None,
            ch if ch.is_whitespace() => {}
            _ => return None,
        }
    }
    None
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// The drawing side of `draw`: the theme paints, the two fonts, and the measurement seam.
struct Painter<'a> {
    measure: &'a dyn TextMeasure,
    label_font: Font,
    head_font: Font,
    line_advance: f64,
    head_max_chars: usize,
    line_color: Option<Color>,
    main_bkg: Option<Color>,
    text_color: Option<Color>,
}

impl Painter<'_> {
    /// `drawMultilineText`: one `<text>` with one `<tspan>` per line, centred on the block and
    /// anchored as CSS resolves the class. Returns the run and the `getBBox()` box upstream
    /// centres the head label and sizes the label box with.
    fn text_run(
        &self,
        text: &str,
        x: f64,
        y: f64,
        anchor: TextAnchor,
        baseline: TextBaseline,
        font: &Font,
    ) -> (TextRun, Rect) {
        let lines = split_lines(text);
        let width = lines
            .iter()
            .map(|line| self.measure.width(line, font))
            .fold(0.0, f64::max);
        let ink_height = self.measure.line_height(font);
        let height = ink_height + self.line_advance * (lines.len().saturating_sub(1)) as f64;
        let first_y = y - (lines.len().saturating_sub(1) as f64 * self.line_advance) / 2.0;

        let block = TextBlock::new(
            lines
                .iter()
                .map(|line| TextLine::new(vec![TextSpan::plain(*line)]))
                .collect(),
            Size::new(width, height),
            self.line_advance,
        );
        let mut run = TextRun::new(Point::new(x, first_y), font.clone(), block);
        run.anchor = anchor;
        run.baseline = baseline;
        run.fill = self.text_color.clone();
        let bounds = run.bounds();
        (run, bounds)
    }

    /// A `<line>` as a two-command path, with the arrow marker when upstream sets one.
    fn line_shape(
        &self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        width: f64,
        marker: bool,
    ) -> PathShape {
        let mut path = Path::new();
        path.move_to(Point::new(x1, y1));
        path.line_to(Point::new(x2, y2));
        let style = PathStyle {
            fill: None,
            stroke: self.line_color.clone(),
            stroke_width: width,
            ..PathStyle::default()
        };
        let mut shape = PathShape::new(path, style);
        if marker && let Some(color) = &self.line_color {
            shape
                .start_markers
                .push(MarkerRef::new(MARKER_ID).with_color(color.clone()));
        }
        shape
    }

    /// `drawHead`: the label, the pointed head path it sits in, and the group that is later
    /// moved to the spine's left end.
    fn head(&self, spine_y: f64, label: &str) -> Group {
        let text = wrap_text(label, self.head_max_chars);
        let (run, text_box) = self.text_run(
            &text,
            0.0,
            0.0,
            TextAnchor::Middle,
            TextBaseline::Middle,
            &self.head_font,
        );

        let width = (text_box.width + 6.0).max(HEAD_MIN_WIDTH);
        let height = (text_box.height * 2.0 + HEAD_MIN_HEIGHT).max(HEAD_MIN_HEIGHT);

        let mut path = Path::new();
        path.move_to(Point::new(0.0, -height / 2.0));
        path.line_to(Point::new(0.0, height / 2.0));
        path.quad_to(Point::new(width * 2.4, 0.0), Point::new(0.0, -height / 2.0));
        path.close();
        let head_style = PathStyle {
            fill: self.main_bkg.clone(),
            stroke: self.line_color.clone(),
            stroke_width: 2.0,
            ..PathStyle::default()
        };

        let translate_x = (width - text_box.width) / 2.0 - text_box.x + 3.0;
        let translate_y = -text_box.y - text_box.height / 2.0;
        let mut run = run;
        run.position.x += translate_x;
        run.position.y += translate_y;

        let mut group = Group::translated(0.0, spine_y).with_class("ishikawa-head-group");
        group.push(PathShape::new(path, head_style));
        group.push(run);
        group
    }

    /// `drawCauseLabel`: the label box and its centred text at the end of a branch.
    fn cause_label(&self, text: &str, x: f64, y: f64, direction: f64) -> (Group, Rect) {
        let (run, text_box) = self.text_run(
            text,
            x,
            y + CAUSE_LABEL_OFFSET * direction,
            TextAnchor::Middle,
            TextBaseline::Middle,
            &self.label_font,
        );
        let label_box = Rect::new(
            text_box.x - LABEL_BOX_PADDING_X,
            text_box.y - LABEL_BOX_PADDING_Y,
            text_box.width + LABEL_BOX_PADDING_X * 2.0,
            text_box.height + LABEL_BOX_PADDING_Y * 2.0,
        );
        let style = PathStyle {
            fill: self.main_bkg.clone(),
            stroke: self.line_color.clone(),
            stroke_width: 2.0,
            ..PathStyle::default()
        };
        let mut group = Group::new().with_class("ishikawa-label-group");
        group.push(RectShape::new(label_box, 0.0, style));
        group.push(run);
        (group, text_box)
    }

    /// `drawBranch`: one cause's branch, its label, and its children's sub-bones. Returns the
    /// leftmost text box in the branch, which becomes the next pair's spine end.
    #[allow(clippy::too_many_arguments)]
    fn branch(
        &self,
        pair: &mut Group,
        node: &IshikawaNode,
        start_x: f64,
        start_y: f64,
        direction: f64,
        length: f64,
        cos_a: f64,
        sin_a: f64,
    ) -> Option<f64> {
        let children = &node.children;
        let line_len = length * if children.is_empty() { 0.2 } else { 1.0 };
        let dx = -cos_a * line_len;
        let dy = sin_a * line_len * direction;
        let end_x = start_x + dx;
        let end_y = start_y + dy;

        pair.push(self.line_shape(start_x, start_y, end_x, end_y, 2.0, true));
        let (label_group, label_box) = self.cause_label(&node.text, end_x, end_y, direction);
        pair.push(label_group);
        let mut min_x = label_box.x;

        if children.is_empty() {
            return Some(min_x);
        }

        let (entries, y_order) = flatten_tree(children, direction);
        let entry_count = entries.len();
        let mut ys = vec![0.0; entry_count];
        for (slot, entry_index) in y_order.iter().enumerate() {
            ys[*entry_index] = start_y + dy * ((slot + 1) as f64 / (entry_count + 1) as f64);
        }

        let mut bones: HashMap<i32, Bone> = HashMap::new();
        bones.insert(
            -1,
            Bone {
                x0: start_x,
                y0: start_y,
                x1: end_x,
                y1: end_y,
                child_count: children.len(),
                children_drawn: 0,
            },
        );

        let diagonal_x = -cos_a;
        let diagonal_y = sin_a * direction;
        for (index, entry) in entries.iter().enumerate() {
            let y = ys[index];
            let parent = *bones
                .get(&entry.parent)
                .expect("a child's parent bone is inserted before the child");
            let mut group = Group::new().with_class("ishikawa-sub-group");
            let bx0;
            let by0;
            let bx1;
            if entry.depth % 2 == 0 {
                // Horizontal bone: attach to the parent's diagonal at the target Y, extend left.
                let parent_dy = parent.y1 - parent.y0;
                bx0 = lerp(
                    parent.x0,
                    parent.x1,
                    if parent_dy != 0.0 {
                        (y - parent.y0) / parent_dy
                    } else {
                        0.5
                    },
                );
                by0 = y;
                bx1 = bx0
                    - if entry.child_count > 0 {
                        BONE_BASE + entry.child_count as f64 * BONE_PER_CHILD
                    } else {
                        BONE_STUB
                    };
                group.push(self.line_shape(bx0, y, bx1, y, 1.0, true));
                let (run, text_box) = self.text_run(
                    &entry.text,
                    bx1,
                    y,
                    TextAnchor::End,
                    TextBaseline::Middle,
                    &self.label_font,
                );
                min_x = min_x.min(text_box.x);
                group.push(run);
            } else {
                // Diagonal bone: start from an evenly spaced point on the parent's horizontal,
                // angle toward the target Y.
                let drawn = {
                    let parent = bones
                        .get_mut(&entry.parent)
                        .expect("a child's parent bone is inserted before the child");
                    let drawn = parent.children_drawn;
                    parent.children_drawn += 1;
                    drawn
                };
                bx0 = lerp(
                    parent.x0,
                    parent.x1,
                    (parent.child_count - drawn) as f64 / (parent.child_count + 1) as f64,
                );
                by0 = parent.y0;
                bx1 = bx0 + diagonal_x * ((y - by0) / diagonal_y);
                group.push(self.line_shape(bx0, by0, bx1, y, 1.0, true));
                let baseline = if direction < 0.0 {
                    TextBaseline::Alphabetic
                } else {
                    TextBaseline::Hanging
                };
                let (run, text_box) = self.text_run(
                    &entry.text,
                    bx1,
                    y,
                    TextAnchor::End,
                    baseline,
                    &self.label_font,
                );
                min_x = min_x.min(text_box.x);
                group.push(run);
            }

            if entry.child_count > 0 {
                bones.insert(
                    index as i32,
                    Bone {
                        x0: bx0,
                        y0: by0,
                        x1: bx1,
                        y1: y,
                        child_count: entry.child_count,
                        children_drawn: 0,
                    },
                );
            }
            pair.push(group);
        }

        Some(min_x)
    }

    /// The `ishikawa-arrow` marker: a 10x10 view box, its tip at the line's start, scaled six
    /// stroke widths wide.
    fn marker_def(&self) -> MarkerDef {
        let mut path = Path::new();
        path.move_to(Point::new(10.0, 0.0));
        path.line_to(Point::new(0.0, 5.0));
        path.line_to(Point::new(10.0, 10.0));
        path.close();
        let style = PathStyle {
            fill: self.line_color.clone(),
            ..PathStyle::default()
        };
        let mut def = MarkerDef::new(MARKER_ID, Rect::new(0.0, 0.0, 10.0, 10.0))
            .with_reference(Point::new(0.0, 5.0))
            .with_size(6.0, 6.0)
            .with_orient(MarkerOrient::Auto)
            .with_class(MARKER_ID);
        def.push(PathShape::new(path, style));
        def
    }
}

/// The box the whole drawing fills: every painted item in diagram coordinates, groups'
/// transforms applied, matching the `getBBox()` `applyPaddedViewBox` pads.
fn scene_bounds(items: &[Item]) -> Option<Rect> {
    fn include(out: &mut Option<Rect>, rect: Rect) {
        *out = Some(out.map_or(rect, |bounds| bounds.union(rect)));
    }

    fn walk(item: &Item, transform: crate::scene::Transform, out: &mut Option<Rect>) {
        match item {
            Item::Group(group) => {
                let transform = transform.then(group.transform);
                for child in &group.items {
                    walk(child, transform, out);
                }
            }
            Item::Path(shape) => {
                if let Some(bounds) = crate::render::pipeline::dagre::svg_path_bounds(&shape.path) {
                    include(out, transformed(bounds, transform));
                }
            }
            Item::Rect(shape) => include(out, transformed(shape.rect, transform)),
            Item::Ellipse(shape) => include(
                out,
                transformed(
                    Rect::new(
                        shape.center.x - shape.radius_x,
                        shape.center.y - shape.radius_y,
                        shape.radius_x * 2.0,
                        shape.radius_y * 2.0,
                    ),
                    transform,
                ),
            ),
            Item::Text(run) => include(out, transformed(run.bounds(), transform)),
            Item::Image(image) => include(out, transformed(image.rect, transform)),
        }
    }

    fn transformed(rect: Rect, transform: crate::scene::Transform) -> Rect {
        let corners = [
            transform.apply(Point::new(rect.x, rect.y)),
            transform.apply(Point::new(rect.right(), rect.y)),
            transform.apply(Point::new(rect.right(), rect.bottom())),
            transform.apply(Point::new(rect.x, rect.bottom())),
        ];
        let mut bounds = Rect::new(corners[0].x, corners[0].y, 0.0, 0.0);
        for corner in &corners[1..] {
            bounds = bounds.union(Rect::new(corner.x, corner.y, 0.0, 0.0));
        }
        bounds
    }

    let mut bounds: Option<Rect> = None;
    for item in items {
        walk(item, crate::scene::Transform::IDENTITY, &mut bounds);
    }
    bounds
}

#[cfg(test)]
mod tests;
