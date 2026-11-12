//! Source-to-[`Scene`] rendering for timelines: the Rust side of `timelineRenderer.ts` (the
//! `LR` layout) and `timelineRendererVertical.ts` (the `TD` layout), plus the drawing helpers in
//! `svgDraw.js` and the section styles in `styles.js`.
//!
//! A timeline does not go through the unified layout pipeline: both renderers draw with plain d3.
//! `draw` reads `getSections()`/`getTasks()`/`getCommonDb().getDiagramTitle()` from
//! [`Ast`](super::Ast), measures every label with `svgDraw.getVirtualNodeHeight` (the same
//! `wrap` + `getBBox().height` rule `drawNode` uses), places section, task and event boxes, joins
//! each task to its stacked events with a dashed marker-ended line, draws the activity line and
//! the title, and fits the viewBox with the configured padding. [`draw`] ports that. [`render`]
//! parses and uses the crate's monospace fallback measurement.
//!
//! The layout constants are upstream's, including the quirk that changes the drawing: `draw`'s
//! task-height precalculation measures the task *object* (`[object Object]`, 17 characters once
//! `wrap` joins its whitespace token), not its label, so an `LR` task box is at least as tall as
//! that string wraps at 150 units (two lines, 84.8 at the default font); the same string fits one
//! line at the vertical variant's 200-unit width, so there it only sets a one-line floor. The
//! `LR` task label and event label are both wrapped at the node's 150-unit width, sections at
//! `200 × max(tasks in section, 1) − 50`, and the vertical variant at 200 (tasks), 300 (events)
//! and 580 (sections).
//!
//! Colors are the theme variables `styles.js` emits as CSS: a node's `.section-N` path fill is
//! `cScale{N+1}` (N is `(fullSection % THEME_COLOR_LIMIT) − 1`), its text fill
//! `cScaleLabel{N+1}`, and its bottom rule `cScaleInv{N+1}` at width 3. The connector and activity
//! lines carry `stroke="black"` attributes upstream, so the scene keeps that paint; the title
//! takes the theme's `textColor`, which the SVG root rule sets. The marker is `M 0,0 V 4 L6,2 Z`
//! at `refX 5`, `refY 2`, `6 × 4`, `orient="auto"`.
//!
//! Text is measured in the `monospace` family, the rule the fixture harness renders with and
//! that [`FallbackMeasure`](crate::measure::FallbackMeasure) reproduces; the theme's family would
//! measure differently from the harness's browser font. `svgDraw.wrap` is ported literally,
//! including its whitespace tokens: `"a b"` wraps to `"a   b"` in the emitted `<tspan>` because
//! each separator is its own token joined by a space.
//!
//! Where the port stops short of upstream: the classes (`timeline-node section--1`,
//! `taskWrapper`, `lineWrapper`, `node-bkg`) key only CSS, and a [`Scene`] carries paints rather
//! than selectors, so the styles those rules declare are resolved into the shapes. The `redux`
//! themes' zero-radius background and shifted label are kept; `look: 'neo'`'s `data-look`
//! attribute, gradient and drop shadow are CSS the painter applies. The `LR` renderer computes
//! `maxEventCount` and never reads it, so it is not ported.

use serde_json::{Map, Value};

use crate::config::Config;
use crate::error::ParseError;
use crate::measure::{Font, TextMeasure};
use crate::scene::{
    Color, Group, Item, MarkerDef, MarkerOrient, MarkerRef, Path, PathCommand, PathShape,
    PathStyle, Point, Rect, Scene, Size, TextAnchor, TextBaseline, TextBlock, TextLine, TextRun,
    TextSpan, Transform,
};

use super::Ast;
use super::ast::{Direction, Task};

/// `svgDraw.drawNode`'s corner radius (`defaultBkg`'s `r`) for every theme but `redux`.
const NODE_RADIUS: f64 = 5.0;
/// `defaultBkg`'s `rd`, the height of the straight left and right edges.
const NODE_EDGE: f64 = 5.0;
/// `timelineRendererVertical.ts`'s `NODE_WIDTH`.
const NODE_WIDTH: f64 = 200.0;
/// `timelineRendererVertical.ts`'s `NODE_PADDING`.
const NODE_PADDING: f64 = 5.0;
/// `EVENT_WIDTH = NODE_WIDTH + 100`.
const EVENT_WIDTH: f64 = NODE_WIDTH + 100.0;
/// `TASK_AXIS_GAP`.
const TASK_AXIS_GAP: f64 = 20.0;
/// `TASK_VERTICAL_GAP`.
const TASK_VERTICAL_GAP: f64 = 30.0;
/// `SECTION_TASK_GAP`.
const SECTION_TASK_GAP: f64 = 20.0;
/// `EVENT_SPACING`.
const EVENT_SPACING: f64 = 10.0;
/// `EVENT_VERTICAL_GAP`.
const EVENT_VERTICAL_GAP: f64 = 0.0;
/// `EVENT_AXIS_GAP`.
const EVENT_AXIS_GAP: f64 = 50.0;
/// The label `wrap` wraps the precalculated task object to (`String({})`).
const TASK_OBJECT_TEXT: &str = "[object Object]";

/// Renders a timeline body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(body: &str, config: &Config, measure: &dyn TextMeasure) -> Result<Scene, ParseError> {
    let ast = super::parse(body)?;
    Ok(draw_ast(&ast, config, measure))
}

/// Renders an already parsed timeline database.
pub fn draw_ast(ast: &Ast, config: &Config, measure: &dyn TextMeasure) -> Scene {
    let ctx = Ctx::new(config, measure);
    match ast.direction {
        Direction::Lr => draw_horizontal(ast, &ctx),
        Direction::Td => draw_vertical(ast, &ctx),
    }
}

/// Renders a timeline body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, ParseError> {
    draw(body, config, &crate::measure::FallbackMeasure)
}

/// The render-time configuration and text system both renderers read, plus the resolved theme
/// palette.
struct Ctx<'a> {
    config: &'a Config,
    measure: &'a dyn TextMeasure,
    font: Font,
    font_size: f64,
    theme_variables: &'a Map<String, Value>,
    color_limit: usize,
}

impl<'a> Ctx<'a> {
    fn new(config: &'a Config, measure: &'a dyn TextMeasure) -> Self {
        let font_size = config.font_size;
        let color_limit = config
            .theme_variables
            .get("THEME_COLOR_LIMIT")
            .and_then(Value::as_f64)
            .filter(|limit| *limit >= 1.0)
            .map(|limit| limit as usize)
            .unwrap_or(12);
        Self {
            config,
            measure,
            font: Font::new("monospace", font_size),
            font_size,
            theme_variables: &config.theme_variables,
            color_limit,
        }
    }

    fn left_margin(&self) -> f64 {
        self.config.timeline.left_margin
    }

    fn padding(&self) -> f64 {
        self.config.timeline.padding.unwrap_or(50.0)
    }

    fn disable_multicolor(&self) -> bool {
        self.config.timeline.disable_multicolor
    }

    fn is_redux_theme(&self) -> bool {
        self.config.theme.contains("redux")
    }

    fn is_neo_look(&self) -> bool {
        self.config.look == "neo"
    }

    /// The section class upstream derives from `fullSection`: `(fullSection % limit) - 1`.
    fn section_index(&self, full_section: usize) -> i64 {
        (full_section % self.color_limit) as i64 - 1
    }

    fn theme_color(&self, key: &str, section: i64) -> Option<Color> {
        let index = section + 1;
        if index < 0 {
            return None;
        }
        self.theme_variables
            .get(&format!("{key}{index}"))
            .and_then(Value::as_str)
            .map(Color::new)
    }

    fn section_fill(&self, section: i64) -> Option<Color> {
        self.theme_color("cScale", section)
    }

    fn section_label(&self, section: i64) -> Option<Color> {
        self.theme_color("cScaleLabel", section)
    }

    fn section_line(&self, section: i64) -> Option<Color> {
        self.theme_color("cScaleInv", section)
    }

    fn title_color(&self) -> Option<Color> {
        self.theme_variables
            .get("textColor")
            .and_then(Value::as_str)
            .map(Color::new)
    }

    /// `svgDraw.wrap`'s lines for `text` at `width`, measured in the monospace rule.
    fn wrap_lines(&self, text: &str, width: f64) -> Vec<String> {
        let tokens = split_js_tokens(text);
        let mut line: Vec<String> = Vec::new();
        let mut tspans: Vec<String> = Vec::new();
        let mut current = String::new();
        for word in tokens {
            line.push(word.clone());
            current = join_trimmed(&line);
            if self.measure.width(&current, &self.font) > width || word == "<br>" {
                line.pop();
                current = join_trimmed(&line);
                if word == "<br>" {
                    line = vec![String::new()];
                } else {
                    line = vec![word.clone()];
                }
                tspans.push(std::mem::take(&mut current));
                current = word;
            }
        }
        tspans.push(current);
        tspans
    }

    /// The block's ink height: one 1.15em line plus a 1.1em advance per following line.
    fn block_height(&self, lines: &[String]) -> f64 {
        if !lines.iter().any(|line| !line.is_empty()) {
            return 0.0;
        }
        self.font_size * 1.15 + self.font_size * 1.1 * (lines.len() as f64 - 1.0)
    }

    /// `getVirtualNodeHeight`: the wrapped block's height plus half a line of leading and the
    /// node's padding.
    fn virtual_node_height(&self, text: &str, width: f64, padding: f64) -> f64 {
        let lines = self.wrap_lines(text, width);
        self.block_height(&lines) + self.font_size * 1.1 * 0.5 + padding
    }

    /// `drawNode`: the section-classed node group with its background path, bottom rule and
    /// centered wrapped text. Returns the group, its padded width, and its height.
    fn draw_node(
        &self,
        text: &str,
        full_section: usize,
        width: f64,
        padding: f64,
        max_height: f64,
        is_event: bool,
    ) -> DrawnNode {
        let section = self.section_index(full_section);
        let lines = self.wrap_lines(text, width);
        let block_height = self.block_height(&lines);
        let height = (block_height + self.font_size * 1.1 * 0.5 + padding).max(max_height);
        let node_width = width + 2.0 * padding;

        let measured_width = lines
            .iter()
            .map(|line| self.measure.width(line, &self.font))
            .fold(0.0, f64::max);
        let text_block = TextBlock::new(
            lines
                .iter()
                .map(|line| TextLine::new(vec![TextSpan::plain(line.clone())]))
                .collect(),
            Size::new(measured_width, block_height),
            self.font_size * 1.1,
        );
        let text_y = if self.is_redux_theme() {
            if is_event {
                padding / 2.0 + 3.0
            } else {
                padding
            }
        } else {
            padding / 2.0
        };
        let mut text_group = Group::translated(node_width / 2.0, text_y);
        let mut run = TextRun::new(Point::ZERO, self.font.clone(), text_block);
        run.anchor = TextAnchor::Middle;
        run.baseline = TextBaseline::Top;
        run.fill = self.section_label(section).or_else(|| self.title_color());
        run.wrapping_width = Some(width);
        text_group.push(run);

        let mut background = Group::new();
        background.push(self.background_path(node_width, height, section));
        if !self.is_redux_theme() {
            background.push(self.bottom_rule(node_width, height, section));
        }

        let mut node = Group::new();
        node.class = Some(format!("timeline-node section-{section}"));
        node.push(background);
        node.push(text_group);
        DrawnNode {
            group: node,
            width: node_width,
            height,
        }
    }

    /// `defaultBkg`'s rounded background path, or the redux theme's sharp one.
    fn background_path(&self, width: f64, height: f64, section: i64) -> PathShape {
        let mut path = Path::new();
        if self.is_redux_theme() {
            path.move_to(Point::new(0.0, height - NODE_EDGE));
            path.line_to(Point::new(0.0, 0.0));
            path.line_to(Point::new(width, 0.0));
            path.line_to(Point::new(width, height));
            path.line_to(Point::new(0.0, height));
            path.close();
        } else {
            let r = NODE_RADIUS;
            path.move_to(Point::new(0.0, height - NODE_EDGE));
            path.line_to(Point::new(0.0, NODE_EDGE));
            path.quad_to(Point::new(0.0, 0.0), Point::new(r, 0.0));
            path.line_to(Point::new(width - r, 0.0));
            path.quad_to(Point::new(width, 0.0), Point::new(width, r));
            path.line_to(Point::new(width, height));
            path.line_to(Point::new(0.0, height));
            path.close();
        }
        // SVG's default paint is black, which is what a section without a generated
        // `.section-N path` rule falls back to when the theme has fewer colors than sections.
        PathShape::new(
            path,
            PathStyle {
                fill: self
                    .section_fill(section)
                    .or_else(|| Some(Color::new("#000000"))),
                ..PathStyle::default()
            },
        )
    }

    /// The `.section-N line` rule under the background: `cScaleInv{N+1}` at width 3.
    fn bottom_rule(&self, width: f64, height: f64, section: i64) -> PathShape {
        let mut path = Path::new();
        path.move_to(Point::new(0.0, height));
        path.line_to(Point::new(width, height));
        PathShape::new(
            path,
            PathStyle {
                fill: None,
                stroke: self.section_line(section),
                stroke_width: 3.0,
                ..PathStyle::default()
            },
        )
    }

    /// One dashed, marker-ended connector line.
    fn connector(&self, x1: f64, y1: f64, x2: f64, y2: f64, width: f64, dash: bool) -> PathShape {
        let mut path = Path::new();
        path.move_to(Point::new(x1, y1));
        path.line_to(Point::new(x2, y2));
        let mut style = PathStyle::stroked("black", width);
        if dash {
            style = style.with_dash([5.0, 5.0]);
        }
        PathShape::new(path, style)
            .with_end_marker(MarkerRef::new(format!("{}-arrowhead", DIAGRAM_ID)))
    }
}

/// The diagram id this renderer prefixes its marker ids with. Upstream uses the id
/// `mermaid.render` was called with; the scene id is only namespacing for the painter.
const DIAGRAM_ID: &str = "timeline";

struct DrawnNode {
    group: Group,
    width: f64,
    height: f64,
}

/// `timelineRenderer.ts`'s `draw`, the `LR` layout.
fn draw_horizontal(ast: &Ast, ctx: &Ctx) -> Scene {
    let left_margin = ctx.left_margin();
    let mut master_x = 50.0 + left_margin;
    let section_begin_y = 50.0;
    let mut master_y = section_begin_y;
    let mut max_section_height: f64 = 0.0;
    let mut max_task_height: f64 = 0.0;
    let mut items: Vec<Item> = Vec::new();

    for section in &ast.sections {
        let height = ctx.virtual_node_height(section, 150.0, 20.0);
        max_section_height = max_section_height.max(height + 20.0);
    }

    for _ in &ast.tasks {
        let height = ctx.virtual_node_height(TASK_OBJECT_TEXT, 150.0, 20.0);
        max_task_height = max_task_height.max(height + 20.0);
    }

    let mut max_event_line_length: f64 = 0.0;
    for task in &ast.tasks {
        let mut length: f64 = 0.0;
        for event in &task.events {
            length += ctx.virtual_node_height(event, 150.0, 20.0);
        }
        if !task.events.is_empty() {
            length += (task.events.len() as f64 - 1.0) * EVENT_SPACING;
        }
        max_event_line_length = max_event_line_length.max(length);
    }

    let has_sections = !ast.sections.is_empty();
    let mut section_number = 0usize;
    if has_sections {
        for section in &ast.sections {
            let tasks: Vec<&Task> = ast
                .tasks
                .iter()
                .filter(|task| &task.section == section)
                .collect();
            let count = tasks.len().max(1);
            let width = 200.0 * count as f64 - 50.0;
            let node = ctx.draw_node(
                section,
                section_number,
                width,
                20.0,
                max_section_height,
                false,
            );
            let mut wrapper = Group::translated(master_x, section_begin_y);
            wrapper.push(node.group);
            items.push(Item::Group(wrapper));

            master_y += max_section_height + 50.0;
            if !tasks.is_empty() {
                draw_tasks_horizontal(
                    &mut items,
                    ctx,
                    &tasks,
                    section_number,
                    master_x,
                    master_y,
                    max_task_height,
                    max_event_line_length,
                    false,
                );
            }
            master_x += 200.0 * count as f64;
            master_y = section_begin_y;
            section_number += 1;
        }
    } else {
        let tasks: Vec<&Task> = ast.tasks.iter().collect();
        draw_tasks_horizontal(
            &mut items,
            ctx,
            &tasks,
            section_number,
            master_x,
            master_y,
            max_task_height,
            max_event_line_length,
            true,
        );
    }

    let content = bounds_of(&items).unwrap_or(Rect::ZERO);
    let title = ast.diagram_title.as_str();
    if !title.is_empty() {
        let x = if ctx.is_neo_look() {
            content.x * 2.0 + left_margin
        } else {
            content.width / 2.0 - left_margin
        };
        items.push(title_run(ctx, title, x));
    }

    let depth_y = if has_sections {
        max_section_height + max_task_height + 150.0
    } else {
        max_task_height + 100.0
    };
    let mut axis = Group::new();
    axis.class = Some("lineWrapper".to_string());
    axis.push(ctx.connector(
        left_margin,
        depth_y,
        content.width + 3.0 * left_margin,
        depth_y,
        4.0,
        false,
    ));
    items.push(Item::Group(axis));

    scene_from(items, ctx)
}

/// `drawTasks`, the `LR` task row.
#[allow(clippy::too_many_arguments)]
fn draw_tasks_horizontal(
    items: &mut Vec<Item>,
    ctx: &Ctx,
    tasks: &[&Task],
    section_color: usize,
    master_x: f64,
    master_y: f64,
    mut max_task_height: f64,
    max_event_line_length: f64,
    is_without_sections: bool,
) {
    let mut x = master_x;
    let mut color = section_color;
    for task in tasks {
        let node = ctx.draw_node(&task.task, color, 150.0, 20.0, max_task_height, false);
        max_task_height = max_task_height.max(node.height);
        let mut wrapper = Group::translated(x, master_y);
        wrapper.class = Some("taskWrapper".to_string());
        wrapper.push(node.group);
        items.push(Item::Group(wrapper));

        // `task.events` is always an array upstream, so the connector is drawn even when the
        // task has no events; only its length depends on them.
        draw_events_horizontal(items, ctx, &task.events, color, x, master_y + 100.0);
        let mut line = Group::new();
        line.class = Some("lineWrapper".to_string());
        line.push(ctx.connector(
            x + 190.0 / 2.0,
            master_y + max_task_height,
            x + 190.0 / 2.0,
            master_y + max_task_height + 100.0 + max_event_line_length + 100.0,
            2.0,
            true,
        ));
        items.push(Item::Group(line));

        x += 200.0;
        if is_without_sections && !ctx.disable_multicolor() {
            color += 1;
        }
    }
}

/// `drawEvents`, the `LR` event stack. Upstream adds the stack's height to the connector's
/// length but the connector is drawn from `maxEventLineLength`, so the return value is unused.
fn draw_events_horizontal(
    items: &mut Vec<Item>,
    ctx: &Ctx,
    events: &[String],
    section_color: usize,
    master_x: f64,
    master_y: f64,
) -> f64 {
    let mut height: f64 = 0.0;
    let mut y = master_y + 100.0;
    for event in events {
        let node = ctx.draw_node(event, section_color, 150.0, 20.0, 50.0, true);
        let event_height = node.height;
        height += event_height;
        let mut wrapper = Group::translated(master_x, y);
        wrapper.class = Some("eventWrapper".to_string());
        wrapper.push(node.group);
        items.push(Item::Group(wrapper));
        y += 10.0 + event_height;
    }
    height
}

/// `timelineRendererVertical.ts`'s `draw`, the `TD` layout.
fn draw_vertical(ast: &Ast, ctx: &Ctx) -> Scene {
    let left_margin = ctx.left_margin();
    let master_x = 50.0 + left_margin;
    let mut master_y = 50.0;
    let content_top_y = master_y;
    let section_begin_x = master_x;
    let left_width = NODE_WIDTH + 2.0 * NODE_PADDING + TASK_AXIS_GAP;
    let right_width = EVENT_WIDTH + 2.0 * NODE_PADDING + EVENT_AXIS_GAP;
    let axis_x = section_begin_x + left_width;
    let has_sections = !ast.sections.is_empty();
    let timeline_x = if has_sections {
        axis_x
    } else {
        master_x + left_width
    };
    let section_width = (left_width + right_width - NODE_PADDING * 2.0).max(50.0);
    let mut items: Vec<Item> = Vec::new();

    let mut max_section_height: f64 = 0.0;
    for section in &ast.sections {
        let height = ctx.virtual_node_height(section, section_width, NODE_PADDING);
        max_section_height = max_section_height.max(height);
    }

    let mut max_task_height: f64 = 0.0;
    for _ in &ast.tasks {
        let height = ctx.virtual_node_height(TASK_OBJECT_TEXT, NODE_WIDTH, NODE_PADDING);
        max_task_height = max_task_height.max(height);
    }

    let mut max_event_stack_height: f64 = 0.0;
    for task in &ast.tasks {
        let mut stack: f64 = 0.0;
        for event in &task.events {
            stack += ctx.virtual_node_height(event, EVENT_WIDTH, NODE_PADDING);
        }
        if !task.events.is_empty() {
            stack += (task.events.len() as f64 - 1.0) * EVENT_SPACING;
        }
        max_event_stack_height = max_event_stack_height.max(stack) + EVENT_VERTICAL_GAP;
    }

    let task_block_height = max_task_height.max(max_event_stack_height);
    let task_spacing = task_block_height + TASK_VERTICAL_GAP;

    let mut section_number = 0usize;
    if has_sections {
        for section in &ast.sections {
            let tasks: Vec<&Task> = ast
                .tasks
                .iter()
                .filter(|task| &task.section == section)
                .collect();
            let node = ctx.draw_node(
                section,
                section_number,
                section_width,
                NODE_PADDING,
                max_section_height,
                false,
            );
            let mut wrapper = Group::translated(timeline_x - left_width, master_y);
            wrapper.push(node.group);
            items.push(Item::Group(wrapper));

            let task_start_y = master_y + node.height + SECTION_TASK_GAP;
            if !tasks.is_empty() {
                draw_tasks_vertical(
                    &mut items,
                    ctx,
                    &tasks,
                    section_number,
                    timeline_x,
                    task_start_y,
                    max_task_height,
                    task_spacing,
                    false,
                );
            }

            let task_count = tasks.len();
            let section_height =
                node.height + SECTION_TASK_GAP + task_spacing * task_count.max(1) as f64
                    - if task_count > 0 {
                        TASK_VERTICAL_GAP * 2.0
                    } else {
                        0.0
                    };
            master_y += section_height;
            section_number += 1;
        }
    } else {
        let tasks: Vec<&Task> = ast.tasks.iter().collect();
        draw_tasks_vertical(
            &mut items,
            ctx,
            &tasks,
            section_number,
            timeline_x,
            master_y,
            max_task_height,
            task_spacing,
            true,
        );
    }

    let mut content = bounds_of(&items).unwrap_or(Rect::ZERO);
    let title = ast.diagram_title.as_str();
    if !title.is_empty() {
        let x = content.width / 2.0 - left_margin;
        items.push(title_run(ctx, title, x));
        content = bounds_of(&items).unwrap_or(Rect::ZERO);
    }

    let arrow_top_offset = ctx.font_size * 2.0;
    let arrow_bottom_padding = ctx.font_size * 0.5 + 20.0;
    let mut axis = Group::new();
    axis.class = Some("lineWrapper".to_string());
    axis.push(ctx.connector(
        timeline_x,
        content_top_y - arrow_top_offset,
        timeline_x,
        content.y + content.height + arrow_bottom_padding,
        4.0,
        false,
    ));
    // `lineWrapper.lower()`: the axis is behind every node.
    items.insert(0, Item::Group(axis));

    scene_from(items, ctx)
}

/// `drawTasks`, the `TD` task column.
#[allow(clippy::too_many_arguments)]
fn draw_tasks_vertical(
    items: &mut Vec<Item>,
    ctx: &Ctx,
    tasks: &[&Task],
    section_color: usize,
    timeline_x: f64,
    mut master_y: f64,
    mut max_task_height: f64,
    task_spacing: f64,
    is_without_sections: bool,
) {
    let mut color = section_color;
    for task in tasks {
        let node = ctx.draw_node(
            &task.task,
            color,
            NODE_WIDTH,
            NODE_PADDING,
            max_task_height,
            false,
        );
        let task_x = timeline_x - TASK_AXIS_GAP - node.width;
        let mut wrapper = Group::translated(task_x, master_y);
        wrapper.class = Some("taskWrapper".to_string());
        wrapper.push(node.group);
        items.push(Item::Group(wrapper));
        max_task_height = max_task_height.max(node.height);

        if !task.events.is_empty() {
            draw_events_vertical(
                items,
                ctx,
                &task.events,
                color,
                timeline_x,
                timeline_x + EVENT_AXIS_GAP,
                master_y,
            );
        }

        master_y += task_spacing;
        if is_without_sections && !ctx.disable_multicolor() {
            color += 1;
        }
    }
}

/// `drawEvents`, the `TD` event column and its connectors.
fn draw_events_vertical(
    items: &mut Vec<Item>,
    ctx: &Ctx,
    events: &[String],
    section_color: usize,
    axis_x: f64,
    events_x: f64,
    start_y: f64,
) {
    let mut current_y = start_y;
    for event in events {
        let node = ctx.draw_node(event, section_color, EVENT_WIDTH, NODE_PADDING, 0.0, false);
        let event_height = node.height;
        let mut wrapper = Group::translated(events_x, current_y);
        wrapper.class = Some("eventWrapper".to_string());
        wrapper.push(node.group);
        items.push(Item::Group(wrapper));

        let line_y = current_y + event_height / 2.0;
        let mut line = Group::new();
        line.class = Some("lineWrapper".to_string());
        line.push(ctx.connector(axis_x, line_y, events_x, line_y, 2.0, true));
        items.push(Item::Group(line));

        current_y += event_height + EVENT_SPACING;
    }
}

/// `draw`'s title `<text>`: `4ex` bold at `y = 20`, measured at two half-ems per `ex`.
fn title_run(ctx: &Ctx, title: &str, x: f64) -> Item {
    let font_size = ctx.font_size * 2.0;
    let font = Font::new("monospace", font_size).bold();
    let width = ctx.measure.width(title, &font);
    let block = TextBlock::new(
        vec![TextLine::new(vec![TextSpan::plain(title)])],
        Size::new(width, ctx.measure.line_height(&font)),
        font_size * 1.1,
    );
    let mut run = TextRun::new(Point::new(x, 20.0), font, block);
    run.baseline = TextBaseline::Top;
    run.fill = ctx.title_color();
    Item::Text(run)
}

/// Finishes a scene: the fitted viewBox, the arrowhead definition, and the items.
fn scene_from(items: Vec<Item>, ctx: &Ctx) -> Scene {
    let bounds = bounds_of(&items).unwrap_or(Rect::ZERO);
    let mut scene = Scene::fitted(bounds, ctx.padding());
    scene.defs.push(arrowhead());
    scene.items = items;
    scene
}

/// `initGraphics`' arrowhead: `M 0,0 V 4 L6,2 Z` in a `6 × 4` marker. The vertical renderer's
/// `initGraphics(svg)` omits the diagram id, so upstream's `TD` marker is named
/// `undefined-arrowhead` while its lines point at `arrowhead`; the `LR` marker is
/// `<id>-arrowhead`. The scene keeps one definition per render and the `LR` name.
fn arrowhead() -> MarkerDef {
    let mut path = Path::new();
    path.move_to(Point::new(0.0, 0.0));
    path.line_to(Point::new(0.0, 4.0));
    path.line_to(Point::new(6.0, 2.0));
    path.close();
    let mut def = MarkerDef::new(
        format!("{DIAGRAM_ID}-arrowhead"),
        Rect::new(0.0, 0.0, 6.0, 4.0),
    )
    .with_reference(Point::new(5.0, 2.0))
    .with_size(6.0, 4.0)
    .with_orient(MarkerOrient::Auto);
    def.push(PathShape::new(path, PathStyle::filled("black")));
    def
}

/// The bounds of a list of items in their parent's coordinates.
fn bounds_of(items: &[Item]) -> Option<Rect> {
    let mut bounds: Option<Rect> = None;
    for item in items {
        if let Some(rect) = item_bounds(item, Transform::IDENTITY) {
            bounds = Some(match bounds {
                Some(current) => current.union(rect),
                None => rect,
            });
        }
    }
    bounds
}

fn item_bounds(item: &Item, transform: Transform) -> Option<Rect> {
    match item {
        Item::Group(group) => {
            let transform = transform.then(group.transform);
            bounds_of_transformed(&group.items, transform)
        }
        Item::Rect(rect) => Some(transform_rect(transform, rect.rect)),
        Item::Ellipse(ellipse) => Some(transform_rect(
            transform,
            Rect::new(
                ellipse.center.x - ellipse.radius_x,
                ellipse.center.y - ellipse.radius_y,
                ellipse.radius_x * 2.0,
                ellipse.radius_y * 2.0,
            ),
        )),
        Item::Image(image) => Some(transform_rect(transform, image.rect)),
        Item::Text(text) => Some(transform_rect(transform, text.bounds())),
        Item::Path(path) => path_bounds(&path.path).map(|rect| transform_rect(transform, rect)),
    }
}

fn bounds_of_transformed(items: &[Item], transform: Transform) -> Option<Rect> {
    let mut bounds: Option<Rect> = None;
    for item in items {
        if let Some(rect) = item_bounds(item, transform) {
            bounds = Some(match bounds {
                Some(current) => current.union(rect),
                None => rect,
            });
        }
    }
    bounds
}

/// A path's box from its command points and control points. The timeline only draws straight
/// segments and the rounded corner quadratics, whose control points are the box corners, so this
/// is exact for what the renderers emit.
fn path_bounds(path: &Path) -> Option<Rect> {
    let mut min = Point::new(f64::INFINITY, f64::INFINITY);
    let mut max = Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut seen = false;
    for command in &path.commands {
        let mut add = |point: Point| {
            min.x = min.x.min(point.x);
            min.y = min.y.min(point.y);
            max.x = max.x.max(point.x);
            max.y = max.y.max(point.y);
        };
        match *command {
            PathCommand::MoveTo { x, y }
            | PathCommand::LineTo { x, y }
            | PathCommand::QuadTo { x, y, .. }
            | PathCommand::CubicTo { x, y, .. }
            | PathCommand::ArcTo { x, y, .. } => {
                add(Point::new(x, y));
                seen = true;
            }
            PathCommand::Close => {}
        }
        match *command {
            PathCommand::QuadTo { cx, cy, .. } => add(Point::new(cx, cy)),
            PathCommand::CubicTo {
                c1x, c1y, c2x, c2y, ..
            } => {
                add(Point::new(c1x, c1y));
                add(Point::new(c2x, c2y));
            }
            _ => {}
        }
    }
    if seen {
        Some(Rect::new(min.x, min.y, max.x - min.x, max.y - min.y))
    } else {
        None
    }
}

/// The four transformed corners' box.
fn transform_rect(transform: Transform, rect: Rect) -> Rect {
    let corners = [
        transform.apply(Point::new(rect.x, rect.y)),
        transform.apply(Point::new(rect.right(), rect.y)),
        transform.apply(Point::new(rect.x, rect.bottom())),
        transform.apply(Point::new(rect.right(), rect.bottom())),
    ];
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for corner in corners {
        min_x = min_x.min(corner.x);
        min_y = min_y.min(corner.y);
        max_x = max_x.max(corner.x);
        max_y = max_y.max(corner.y);
    }
    Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
}

/// JavaScript's `text.split(/(\s+|<br>)/)`: the text between matches and every captured
/// separator, in order, including the empty strings around adjacent separators.
fn split_js_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut last = 0usize;
    let mut index = 0usize;
    while index < text.len() {
        if text[index..].starts_with("<br>") {
            tokens.push(text[last..index].to_string());
            tokens.push("<br>".to_string());
            index += 4;
            last = index;
            continue;
        }
        let Some(ch) = text[index..].chars().next() else {
            break;
        };
        if crate::preprocess::is_js_whitespace(ch) {
            let start = index;
            index += ch.len_utf8();
            while index < text.len() {
                let Some(next) = text[index..].chars().next() else {
                    break;
                };
                if crate::preprocess::is_js_whitespace(next) {
                    index += next.len_utf8();
                } else {
                    break;
                }
            }
            tokens.push(text[last..start].to_string());
            tokens.push(text[start..index].to_string());
            last = index;
            continue;
        }
        index += ch.len_utf8();
    }
    tokens.push(text[last..].to_string());
    tokens
}

fn join_trimmed(line: &[String]) -> String {
    line.join(" ").trim().to_string()
}

#[cfg(test)]
mod tests;
