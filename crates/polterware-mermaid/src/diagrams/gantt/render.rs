//! Source-to-[`Scene`] rendering for gantt charts: the Rust side of
//! `diagrams/gantt/ganttRenderer.js`'s `draw`.
//!
//! A gantt chart does not go through the unified layout pipeline: `draw` uses plain d3. It
//! reads the tasks, sections and calendar settings from `ganttDb.js` (ported in [`super::db`]),
//! builds a d3 time scale over the tasks' span, and appends, in this order, the excluded-day
//! bands, the date grid (bottom axis, and the top axis when enabled), the section background
//! rows, the task bars with their labels, the section titles, the today line, and the title.
//! This module ports that: [`draw`] parses, [`draw_ast`] renders an already parsed database,
//! [`draw_ast_with_today`] pins the clock [`draw`] reads for the today line, and [`render`]
//! uses the crate's monospace fallback measurement.
//!
//! Look: `diagrams/gantt/styles.js` and the attributes `ganttRenderer.js` writes. The design
//! system has no gantt control, so the look is mermaid's own.
//!
//! Behavior: `ganttRenderer.js` is the only consumer of d3-scale's time scale and d3-axis in
//! mermaid, and gpui-kit has no gantt renderer, so there is no second behavior reference: the
//! renderer itself is the model, followed call by call, including d3-time's tick selection
//! (`timeTicks`/`tickInterval`) and `d3-time-format` for the axis labels. The three helpers
//! `ganttRenderer.js` calls that the crate had not ported live here: the time scale
//! (`scaleTime` with `rangeRound`), the axis (`axisBottom`/`axisTop`), and the time scale's
//! automatic ticks. Compaction (`displayMode: 'compact'`) ports `getMaxIntersections`, which
//! reassigns `order` and so the row a task lands on.
//!
//! Where the port stops short of upstream, the difference is in the painter-facing scene:
//!
//! * The renderer reads the container width from `elem.parentElement.offsetWidth`, which a
//!   library cannot know; the scene uses `gantt.useWidth` and falls back to upstream's 1200.
//! * Dates are wall-clock with no zone (see [`super::date`]), so the time scale and the ticks
//!   are UTC-wall-clock arithmetic. A browser uses the local zone; with a fixed offset the
//!   positions are the same, and the fixture harness pins a fixed-offset wall clock.
//! * CSS classes become painted shapes: `styles.js` resolves each element's `fill`, `stroke`,
//!   `opacity`, font size and anchor, and the milestone's `transform: rotate(45deg)
//!   scale(0.8,0.8)` around the rect's `transform-origin` becomes a group transform. CSS
//!   transitions, cursors and `shape-rendering` are the painter's business.
//! * The `<a>` wrapper around a linked task (`securityLevel: 'sandbox'`) and the DOM link
//!   callbacks are not modelled; the `clickable` class's colours and weight are.
//! * The axis' `<line>`/`<path>` stroke and the tick text's stroke follow the CSS cascade in a
//!   browser; the scene paints the grid line in `gridColor` at the CSS opacity 0.8, the tick
//!   text in `textColor`, and the zero-width `.grid path` domain as an unpainted path.
//! * SVG ids (`section`, `titleText`, `gantt-exclude-...`) and the `transform-origin`
//!   attribute (only consulted by CSS transitions) are not carried; the source-less scene
//!   keeps its groups in upstream's order.
//! * `configureSvgSize` sizes the `<svg>` element, not the drawing; the scene carries the
//!   `0 0 width height` viewBox and the painter sizes the page, so `useMaxWidth` is the
//!   painter's.
//! * An empty title is not emitted as an empty run.

use serde_json::Value;

use crate::config::{Config, GanttConfig};
use crate::error::ParseError;
use crate::measure::{Font, TextMeasure};
use crate::scene::{
    Color, Group, Path, PathShape, PathStyle, Point, Rect, RectShape, Scene, Size, TextAnchor,
    TextBlock, TextLine, TextRun, TextSpan, Transform,
};

use super::ast::{Ast, Task};
use super::date::{self, Date};
use super::db;

/// `draw`'s fallback width when the container reports none.
const DEFAULT_WIDTH: f64 = 1200.0;
/// `draw`'s cap on custom-tick counts, `MAX_TICK_COUNT`.
const MAX_TICK_COUNT: f64 = 10_000.0;
/// `drawRects`' bar corner radius (`rx`/`ry` = 3).
const BAR_RADIUS: f64 = 3.0;
/// d3-axis' default `tickPadding`.
const TICK_PADDING: f64 = 3.0;
/// d3-axis' pixel offset (a half pixel when `devicePixelRatio <= 1`, as in the harness).
const AXIS_OFFSET: f64 = 0.5;
/// The `font-size` attribute `makeGrid` writes on the axis labels.
const AXIS_FONT_SIZE: f64 = 10.0;
/// `.titleText`'s `font-size: 18px`.
const TITLE_FONT_SIZE: f64 = 18.0;
/// `.vertText`'s `font-size: 15px`.
const VERT_TEXT_FONT_SIZE: f64 = 15.0;
/// d3-time's duration constants, in milliseconds (`duration.js`).
const DURATION_SECOND: f64 = 1_000.0;
const DURATION_MINUTE: f64 = 60_000.0;
const DURATION_HOUR: f64 = 3_600_000.0;
const DURATION_DAY: f64 = 86_400_000.0;
const DURATION_WEEK: f64 = 604_800_000.0;
const DURATION_MONTH: f64 = 2_592_000_000.0;
const DURATION_YEAR: f64 = 31_536_000_000.0;

/// Renders a gantt body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(body: &str, config: &Config, measure: &dyn TextMeasure) -> Result<Scene, ParseError> {
    let ast = super::parse(body)?;
    Ok(draw_ast(&ast, config, measure))
}

/// Renders an already parsed gantt database, with the clock's wall clock for the today line.
pub fn draw_ast(ast: &Ast, config: &Config, measure: &dyn TextMeasure) -> Scene {
    draw_ast_with_today(ast, config, measure, Date::now())
}

/// Renders an already parsed database with a pinned "now", the date `drawToday` reads from
/// `new Date()`.
pub fn draw_ast_with_today(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
    today: Date,
) -> Scene {
    draw_ast_with_font(ast, config, measure, today, "monospace")
}

/// Renders a gantt body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, ParseError> {
    draw(body, config, &crate::measure::FallbackMeasure)
}

/// The axis a grid is drawn for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum AxisSide {
    Bottom,
    Top,
}

fn draw_ast_with_font(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
    today: Date,
    family: &str,
) -> Scene {
    let gantt = &config.gantt;
    let theme = &config.theme_variables;
    let width = gantt.use_width.unwrap_or(DEFAULT_WIDTH);
    let top = gantt.top_padding;
    let left = gantt.left_padding;
    let right = gantt.right_padding;
    let bar_height = gantt.bar_height;
    let gap = bar_height + gantt.bar_gap;

    // `getTasks()` records are mutated by `getMaxIntersections` (compact mode) and sorted by
    // start time below; keep the database's own order elsewhere.
    let mut tasks: Vec<Task> = ast.tasks.clone();
    let task_count = tasks.len();
    let mut order_of: Vec<usize> = (0..task_count).collect();

    // `categories` is every distinct `type` in task order, vert markers included.
    let mut categories: Vec<String> = Vec::new();
    for task in tasks.iter().filter(|task| !task.vert) {
        if !categories.contains(&task.task_type) {
            categories.push(task.task_type.clone());
        }
    }

    let without_vert: Vec<usize> = order_of
        .iter()
        .copied()
        .filter(|index| !tasks[*index].vert)
        .collect();

    // Height and per-category row counts, in upstream's two display modes.
    let mut height = 2.0 * top;
    let mut category_heights: Vec<(String, f64)> = Vec::new();
    if ast_display_mode_is_compact(gantt) {
        let mut category_elements: Vec<(String, Vec<usize>)> = Vec::new();
        for index in &without_vert {
            let key = tasks[*index].section.clone();
            match category_elements.iter_mut().find(|(name, _)| *name == key) {
                Some((_, list)) => list.push(*index),
                None => category_elements.push((key, vec![*index])),
            }
        }
        let mut intersections = 0.0;
        for (key, list) in category_elements {
            let category_height = get_max_intersections(&mut tasks, &list, intersections) + 1.0;
            intersections += category_height;
            height += category_height * gap;
            category_heights.push((key, category_height));
        }
    } else {
        height += without_vert.len() as f64 * gap;
        for category in &categories {
            let count = tasks
                .iter()
                .filter(|task| !task.vert && &task.task_type == category)
                .count();
            category_heights.push((category.clone(), count as f64));
        }
    }

    // The time scale's domain is the first start and the last end over every task.
    let mut min_time: Option<Date> = None;
    let mut max_time: Option<Date> = None;
    for task in &tasks {
        if let Some(start) = task.start_time
            && min_time.is_none_or(|current| start < current)
        {
            min_time = Some(start);
        }
        if let Some(end) = task.end_time
            && max_time.is_none_or(|current| end > current)
        {
            max_time = Some(end);
        }
    }
    let start_time = min_time.unwrap_or_else(Date::now);
    let end_time = max_time.unwrap_or(start_time);
    let scale = TimeScale::new(
        start_time.epoch_milliseconds() as f64,
        end_time.epoch_milliseconds() as f64,
        0.0,
        width - left - right,
    );

    // `taskArray.sort(taskCompare)`: stable by start time.
    order_of.sort_by(|a, b| {
        let (a, b) = (tasks[*a].start_time, tasks[*b].start_time);
        match (a, b) {
            (Some(a), Some(b)) => a.cmp(&b),
            _ => std::cmp::Ordering::Equal,
        }
    });

    let mut root = Group::default();

    if !ast.excludes.is_empty() || !ast.includes.is_empty() {
        draw_exclude_days(
            &mut root, &tasks, &order_of, ast, gantt, &scale, height, theme, left,
        );
    }

    make_grid(
        &mut root, ast, gantt, &scale, height, theme, family, measure, left, top,
    );

    draw_rects(
        &mut root,
        &tasks,
        &mut order_of,
        &categories,
        gantt,
        &scale,
        width,
        bar_height,
        gap,
        top,
        left,
        theme,
        family,
        measure,
    );

    vert_labels(
        &mut root,
        &category_heights,
        gantt,
        gap,
        top,
        theme,
        family,
        measure,
    );

    draw_today(&mut root, ast, gantt, &scale, height, today, theme, left);

    if !ast.diagram_title.is_empty() {
        root.push(text_run(
            &ast.diagram_title,
            Point::new(width / 2.0, gantt.title_top_margin),
            TextAnchor::Middle,
            &Font::new(family, TITLE_FONT_SIZE),
            Some(title_color(theme)),
            measure,
        ));
    }

    let mut scene = Scene::new(Rect::new(0.0, 0.0, width, height));
    scene.push(root);
    scene
}

/// `diagObj.db.getDisplayMode() === 'compact' || conf.displayMode === 'compact'`; the
/// database's display mode is only ever set from configuration, so the configuration is the
/// one input.
fn ast_display_mode_is_compact(gantt: &GanttConfig) -> bool {
    gantt.display_mode == "compact"
}

/// `getMaxIntersections`: greedily packs tasks into the first row whose last end is not after
/// the task's start, and writes the row into each task's `order`. Returns the highest row.
fn get_max_intersections(tasks: &mut [Task], indexes: &[usize], order_offset: f64) -> f64 {
    let mut timeline = vec![f64::NEG_INFINITY; indexes.len()];
    let mut sorted = indexes.to_vec();
    sorted.sort_by(|a, b| {
        let (a, b) = (&tasks[*a], &tasks[*b]);
        let start = match (a.start_time, b.start_time) {
            (Some(a), Some(b)) => a.cmp(&b),
            _ => std::cmp::Ordering::Equal,
        };
        start.then(a.order.cmp(&b.order))
    });
    let mut max_intersections = 0.0;
    for index in sorted {
        let start = task_start(&tasks[index]);
        let end = task_end(&tasks[index]);
        for (row, last) in timeline.iter_mut().enumerate() {
            if start >= *last {
                *last = end;
                tasks[index].order = row as i64 + order_offset as i64;
                if row as f64 > max_intersections {
                    max_intersections = row as f64;
                }
                break;
            }
        }
    }
    max_intersections
}

fn task_start(task: &Task) -> f64 {
    task.start_time
        .map(|time| time.epoch_milliseconds() as f64)
        .unwrap_or(f64::INFINITY)
}

fn task_end(task: &Task) -> f64 {
    task.end_time
        .map(|time| time.epoch_milliseconds() as f64)
        .unwrap_or(f64::NEG_INFINITY)
}

/// `drawExcludeDays`: one band per run of excluded calendar days, from the first task's start
/// to the last task's end.
#[allow(clippy::too_many_arguments)]
fn draw_exclude_days(
    root: &mut Group,
    tasks: &[Task],
    order: &[usize],
    ast: &Ast,
    gantt: &GanttConfig,
    scale: &TimeScale,
    height: f64,
    theme: &serde_json::Map<String, Value>,
    left: f64,
) {
    let mut min_time: Option<Date> = None;
    let mut max_time: Option<Date> = None;
    for index in order {
        let task = &tasks[*index];
        if let Some(start) = task.start_time
            && min_time.is_none_or(|current| start < current)
        {
            min_time = Some(start);
        }
        if let Some(end) = task.end_time
            && max_time.is_none_or(|current| end > current)
        {
            max_time = Some(end);
        }
    }
    let (Some(min_time), Some(max_time)) = (min_time, max_time) else {
        return;
    };

    // `dayjs(maxTime).diff(dayjs(minTime), 'year') > 5`.
    let years = whole_years_between(min_time, max_time);
    if years > 5.0 {
        return;
    }

    let mut ranges: Vec<(Date, Date)> = Vec::new();
    let mut range: Option<(Date, Date)> = None;
    let mut day = min_time;
    while day.epoch_milliseconds() <= max_time.epoch_milliseconds() {
        if db::is_invalid_date(
            day,
            &ast.date_format,
            &ast.excludes,
            &ast.includes,
            &ast.weekend,
        ) {
            match range.as_mut() {
                Some((_, end)) => *end = day,
                None => range = Some((day, day)),
            }
        } else if let Some((start, end)) = range.take() {
            ranges.push((start, end));
        }
        day = match day.add(1.0, date::Unit::Day) {
            Some(next) => next,
            None => break,
        };
    }
    if let Some((start, end)) = range.take() {
        ranges.push((start, end));
    }
    if ranges.is_empty() {
        return;
    }

    let mut group = Group::new().with_class("exclude-days");
    let fill = theme_color(theme, "excludeBkgColor").unwrap_or("#eeeeee");
    for (start, end) in ranges {
        let start_of_day = start.midnight();
        let end_of_day = Date::new(end.year, end.month, end.day, 23, 59, 59, 999).unwrap_or(end);
        let x = scale.scale(start_of_day.epoch_milliseconds() as f64) + left;
        let right = scale.scale(end_of_day.epoch_milliseconds() as f64);
        group.push(RectShape::new(
            Rect::new(
                x,
                gantt.grid_line_start_padding,
                right - scale.scale(start_of_day.epoch_milliseconds() as f64),
                height - gantt.top_padding - gantt.grid_line_start_padding,
            ),
            0.0,
            PathStyle::filled(fill),
        ));
    }
    root.push(group);
}

/// `makeGrid`: the bottom axis, and the top axis when `topAxisEnabled()` or `conf.topAxis`.
#[allow(clippy::too_many_arguments)]
fn make_grid(
    root: &mut Group,
    ast: &Ast,
    gantt: &GanttConfig,
    scale: &TimeScale,
    height: f64,
    theme: &serde_json::Map<String, Value>,
    family: &str,
    measure: &dyn TextMeasure,
    left: f64,
    top: f64,
) {
    let axis_format = if !ast.axis_format.is_empty() {
        ast.axis_format.clone()
    } else if ast.date_format == "D" {
        "%d".to_string()
    } else {
        gantt.axis_format.clone()
    };

    let tick_size = -height + top + gantt.grid_line_start_padding;
    let custom = custom_tick_interval(ast, gantt, scale);
    let ticks = axis_ticks(scale, custom.as_ref());

    root.push(grid_group(
        AxisSide::Bottom,
        &ticks,
        scale,
        tick_size,
        left,
        height - 50.0,
        &axis_format,
        theme,
        family,
        measure,
    ));

    if ast.top_axis || gantt.top_axis {
        root.push(grid_group(
            AxisSide::Top,
            &ticks,
            scale,
            tick_size,
            left,
            top,
            &axis_format,
            theme,
            family,
            measure,
        ));
    }
}

/// One `svg.append('g').attr('class','grid')...call(axis)`.
#[allow(clippy::too_many_arguments)]
fn grid_group(
    side: AxisSide,
    ticks: &[Date],
    scale: &TimeScale,
    tick_size: f64,
    side_pad: f64,
    vertical: f64,
    axis_format: &str,
    theme: &serde_json::Map<String, Value>,
    family: &str,
    measure: &dyn TextMeasure,
) -> Group {
    let grid_color = theme_color(theme, "gridColor").unwrap_or("lightgrey");
    let text_color = theme_color(theme, "textColor").unwrap_or("#333");
    let range = scale.range;
    // d3-axis' `k`: 1 for a bottom axis, -1 for a top axis.
    let k = match side {
        AxisSide::Bottom => 1.0,
        AxisSide::Top => -1.0,
    };

    let mut group = Group::translated(side_pad, vertical).with_class("grid");

    // d3-axis draws the domain path first; `.grid path { stroke-width: 0 }` leaves it unpainted.
    let mut domain = Path::new();
    domain.move_to(Point::new(range.0 + AXIS_OFFSET, k * tick_size));
    domain.line_to(Point::new(range.0 + AXIS_OFFSET, AXIS_OFFSET));
    domain.line_to(Point::new(range.1 + AXIS_OFFSET, AXIS_OFFSET));
    domain.line_to(Point::new(range.1 + AXIS_OFFSET, k * tick_size));
    group.push(PathShape::new(
        domain,
        PathStyle {
            fill: None,
            stroke: Some(Color::current()),
            stroke_width: 0.0,
            ..PathStyle::default()
        },
    ));

    for tick in ticks {
        let x = scale.scale(tick.epoch_milliseconds() as f64) + AXIS_OFFSET;
        let mut tick_group = Group::translated(x, 0.0).with_class("tick");

        let mut line = Path::new();
        line.move_to(Point::ZERO);
        line.line_to(Point::new(0.0, k * tick_size));
        let line_style = PathStyle::stroked(grid_color, 1.0).with_opacity(0.8);
        tick_group.push(PathShape::new(line, line_style));

        let dy = match side {
            AxisSide::Bottom => AXIS_FONT_SIZE,
            AxisSide::Top => 0.0,
        };
        tick_group.push(text_run(
            &format_time(*tick, axis_format),
            Point::new(0.0, k * TICK_PADDING + dy),
            TextAnchor::Middle,
            &Font::new(family, AXIS_FONT_SIZE),
            Some(text_color),
            measure,
        ));

        group.push(tick_group);
    }

    group
}

/// The parsed custom tick interval and the weekday its week ticks start on.
struct CustomTicks {
    interval: Interval,
}

/// `reTickInterval.exec(diagObj.db.getTickInterval() || conf.tickInterval)`, after the
/// `MAX_TICK_COUNT` estimate. A rejected interval falls back to d3's automatic ticks.
fn custom_tick_interval(ast: &Ast, gantt: &GanttConfig, scale: &TimeScale) -> Option<CustomTicks> {
    let text = ast
        .tick_interval
        .clone()
        .or_else(|| gantt.tick_interval.clone())?;
    let (every, unit) = parse_tick_interval(&text)?;
    let interval_ms = match unit {
        IntervalUnit::Millisecond => 1.0,
        IntervalUnit::Second => DURATION_SECOND,
        IntervalUnit::Minute => DURATION_MINUTE,
        IntervalUnit::Hour => DURATION_HOUR,
        IntervalUnit::Day => DURATION_DAY,
        IntervalUnit::Week => DURATION_WEEK,
        IntervalUnit::Month => DURATION_MONTH,
        IntervalUnit::Year => DURATION_YEAR,
    };
    let span = scale.max - scale.min;
    let estimated = (span / (every as f64 * interval_ms)).ceil();
    if estimated > MAX_TICK_COUNT {
        return None;
    }
    let weekday = weekday_index(&ast.weekday).unwrap_or(0);
    Some(CustomTicks {
        interval: Interval::every(unit, every as u32, weekday),
    })
}

/// `/^([1-9]\d*)(millisecond|second|minute|hour|day|week|month)$/`.
fn parse_tick_interval(text: &str) -> Option<(i64, IntervalUnit)> {
    let units = [
        ("millisecond", IntervalUnit::Millisecond),
        ("second", IntervalUnit::Second),
        ("minute", IntervalUnit::Minute),
        ("hour", IntervalUnit::Hour),
        ("day", IntervalUnit::Day),
        ("week", IntervalUnit::Week),
        ("month", IntervalUnit::Month),
    ];
    for (name, unit) in units {
        if let Some(prefix) = text.strip_suffix(name)
            && !prefix.is_empty()
            && prefix.bytes().all(|byte| byte.is_ascii_digit())
            && !prefix.starts_with('0')
            && let Ok(value) = prefix.parse::<i64>()
        {
            return Some((value, unit));
        }
    }
    None
}

/// The entries of d3-time's `ticker` table: interval, step, and the duration in ms.
const TICK_INTERVALS: [(IntervalUnit, u32, f64); 18] = [
    (IntervalUnit::Second, 1, DURATION_SECOND),
    (IntervalUnit::Second, 5, 5.0 * DURATION_SECOND),
    (IntervalUnit::Second, 15, 15.0 * DURATION_SECOND),
    (IntervalUnit::Second, 30, 30.0 * DURATION_SECOND),
    (IntervalUnit::Minute, 1, DURATION_MINUTE),
    (IntervalUnit::Minute, 5, 5.0 * DURATION_MINUTE),
    (IntervalUnit::Minute, 15, 15.0 * DURATION_MINUTE),
    (IntervalUnit::Minute, 30, 30.0 * DURATION_MINUTE),
    (IntervalUnit::Hour, 1, DURATION_HOUR),
    (IntervalUnit::Hour, 3, 3.0 * DURATION_HOUR),
    (IntervalUnit::Hour, 6, 6.0 * DURATION_HOUR),
    (IntervalUnit::Hour, 12, 12.0 * DURATION_HOUR),
    (IntervalUnit::Day, 1, DURATION_DAY),
    (IntervalUnit::Day, 2, 2.0 * DURATION_DAY),
    (IntervalUnit::Week, 1, DURATION_WEEK),
    (IntervalUnit::Month, 1, DURATION_MONTH),
    (IntervalUnit::Month, 3, 3.0 * DURATION_MONTH),
    (IntervalUnit::Year, 1, DURATION_YEAR),
];

/// `scale.ticks()` on the time scale: d3-time's `timeTicks`, or a custom interval.
fn axis_ticks(scale: &TimeScale, custom: Option<&CustomTicks>) -> Vec<Date> {
    let start = scale.min;
    let stop = scale.max;
    if let Some(custom) = custom {
        return custom
            .interval
            .range(from_millis(start), from_millis(stop + 1.0));
    }
    if start == stop {
        return vec![from_millis(start)];
    }
    let (low, high) = if stop < start {
        (stop, start)
    } else {
        (start, stop)
    };
    let target = (high - low) / 10.0;
    let mut index = TICK_INTERVALS.len();
    for (position, (_, _, duration)) in TICK_INTERVALS.iter().enumerate() {
        if *duration > target {
            index = position;
            break;
        }
    }
    if index == TICK_INTERVALS.len() {
        let step = tick_step(low / DURATION_YEAR, high / DURATION_YEAR, 10.0);
        return Interval::every(IntervalUnit::Year, js_floor(step).max(1.0) as u32, 0)
            .range(from_millis(low), from_millis(high + 1.0));
    }
    if index == 0 {
        let step = js_max(tick_step(low, high, 10.0), 1.0);
        return Interval::every(IntervalUnit::Millisecond, js_floor(step).max(1.0) as u32, 0)
            .range(from_millis(low), from_millis(high + 1.0));
    }
    let (previous_unit, previous_step, previous_duration) = TICK_INTERVALS[index - 1];
    let (unit, step, duration) = TICK_INTERVALS[index];
    let chosen = if target / previous_duration < duration / target {
        (previous_unit, previous_step)
    } else {
        (unit, step)
    };
    Interval::every(chosen.0, chosen.1, 0).range(from_millis(low), from_millis(high + 1.0))
}

fn from_millis(milliseconds: f64) -> Date {
    Date::from_epoch_milliseconds(milliseconds as i64).unwrap_or_else(Date::now)
}

/// d3-array's `tickStep`.
fn tick_step(start: f64, stop: f64, count: f64) -> f64 {
    let step0 = (stop - start).abs() / js_max(0.0, count);
    let mut step1 = 10f64.powf(js_floor(step0.log10()));
    let error = step0 / step1;
    // Math.sqrt(50), Math.sqrt(10), Math.sqrt(2), d3-array's constants.
    let e10 = 50f64.sqrt();
    let e5 = 10f64.sqrt();
    let e2 = 2f64.sqrt();
    if error >= e10 {
        step1 *= 10.0;
    } else if error >= e5 {
        step1 *= 5.0;
    } else if error >= e2 {
        step1 *= 2.0;
    }
    if stop < start { -step1 } else { step1 }
}

/// `drawRects`: the section background rows, then every task's bar and label.
#[allow(clippy::too_many_arguments)]
fn draw_rects(
    root: &mut Group,
    tasks: &[Task],
    order: &mut [usize],
    categories: &[String],
    gantt: &GanttConfig,
    scale: &TimeScale,
    width: f64,
    bar_height: f64,
    gap: f64,
    top: f64,
    left: f64,
    theme: &serde_json::Map<String, Value>,
    family: &str,
    measure: &dyn TextMeasure,
) {
    // `theArray.sort((a, b) => (a.vert === b.vert ? 0 : a.vert ? 1 : -1))`: vert markers last,
    // otherwise the start-time order already in `order`.
    order.sort_by_key(|index| tasks[*index].vert);

    let without_vert: Vec<usize> = order
        .iter()
        .copied()
        .filter(|index| !tasks[*index].vert)
        .collect();

    // One background row per distinct `order`, the first task that has it.
    let mut seen_orders: Vec<i64> = Vec::new();
    let mut unique_tasks: Vec<usize> = Vec::new();
    for index in &without_vert {
        let value = tasks[*index].order;
        if !seen_orders.contains(&value) {
            seen_orders.push(value);
            unique_tasks.push(*index);
        }
    }

    let mut sections = Group::new().with_class("sections");
    for index in unique_tasks {
        let task = &tasks[index];
        let style = section_style(task, categories, gantt, theme);
        sections.push(RectShape::new(
            Rect::new(
                0.0,
                task.order as f64 * gap + top - 2.0,
                width - gantt.right_padding / 2.0,
                gap,
            ),
            0.0,
            style,
        ));
    }
    root.push(sections);

    // The DOM gets every rect first, then every text: d3 appends twice on the same enter
    // selection.
    let mut group = Group::new().with_class("tasks");
    for index in order.iter() {
        let task = &tasks[*index];
        let rect = task_rect(
            task,
            gantt,
            scale,
            gap,
            top,
            left,
            bar_height,
            without_vert.len(),
        );
        let shape = RectShape::new(rect, BAR_RADIUS, task_style(task, theme));
        if task.milestone {
            // `.milestone { transform: rotate(45deg) scale(0.8, 0.8) }` around the rect's
            // `transform-origin`, the way SVG applies a CSS transform to an element.
            let center = transform_origin(task, scale, gap, top, left, bar_height);
            let transform = Transform::translate(-center.x, -center.y)
                .then(Transform::scale(0.8, 0.8))
                .then(Transform::rotate(45.0))
                .then(Transform::translate(center.x, center.y));
            let mut milestone = Group::default()
                .with_class("milestone")
                .with_transform(transform);
            milestone.push(shape);
            group.push(milestone);
        } else {
            group.push(shape);
        }
    }

    let font_size = gantt.font_size;
    for index in order.iter() {
        let task = &tasks[*index];
        let label = task_label(
            task,
            gantt,
            scale,
            width,
            gap,
            top,
            left,
            bar_height,
            without_vert.len(),
            measure,
        );
        let anchor = label.anchor;
        let color = task_text_color(task, label.outside, theme);
        let size = if task.vert {
            VERT_TEXT_FONT_SIZE
        } else {
            font_size
        };
        let mut run = text_run(
            &task.task,
            Point::new(label.x, label.y),
            anchor,
            &Font::new(family, size),
            Some(color.as_str()),
            measure,
        );
        if task.milestone {
            for line in &mut run.lines {
                for span in &mut line.spans {
                    span.italic = true;
                }
            }
        }
        if task.classes.iter().any(|class| class == "clickable") {
            for line in &mut run.lines {
                for span in &mut line.spans {
                    span.bold = true;
                }
            }
        }
        group.push(run);
    }
    root.push(group);
}

/// Where `drawRects` puts one bar, before the milestone's CSS transform.
#[allow(clippy::too_many_arguments)]
fn task_rect(
    task: &Task,
    gantt: &GanttConfig,
    scale: &TimeScale,
    gap: f64,
    top: f64,
    left: f64,
    bar_height: f64,
    without_vert_count: usize,
) -> Rect {
    let start = scale.scale(task_start(task));
    let end = scale.scale(task_end(task));
    let render_end = task
        .render_end_time
        .map(|time| scale.scale(time.epoch_milliseconds() as f64))
        .unwrap_or(end);
    let x = if task.milestone {
        start + left + 0.5 * (end - start) - 0.5 * bar_height
    } else {
        start + left
    };
    let y = if task.vert {
        gantt.grid_line_start_padding
    } else {
        task.order as f64 * gap + top
    };
    let width = if task.milestone {
        bar_height
    } else if task.vert {
        0.08 * bar_height
    } else {
        render_end - start
    };
    let height = if task.vert {
        without_vert_count as f64 * gap + bar_height * 2.0
    } else {
        bar_height
    };
    Rect::new(x, y, width, height)
}

/// The `transform-origin` attribute `drawRects` computes for every bar.
fn transform_origin(
    task: &Task,
    scale: &TimeScale,
    gap: f64,
    top: f64,
    left: f64,
    bar_height: f64,
) -> Point {
    let start = scale.scale(task_start(task));
    let end = scale.scale(task_end(task));
    Point::new(
        start + left + 0.5 * (end - start),
        task.order as f64 * gap + top + 0.5 * bar_height,
    )
}

/// The `x`, `y`, anchor and placement branch of a task's label.
struct TaskLabel {
    x: f64,
    y: f64,
    anchor: TextAnchor,
    outside: bool,
}

#[allow(clippy::too_many_arguments)]
fn task_label(
    task: &Task,
    gantt: &GanttConfig,
    scale: &TimeScale,
    width: f64,
    gap: f64,
    top: f64,
    left: f64,
    bar_height: f64,
    without_vert_count: usize,
    measure: &dyn TextMeasure,
) -> TaskLabel {
    let font_size = gantt.font_size;
    if task.vert {
        return TaskLabel {
            x: scale.scale(task_start(task)) + left,
            y: gantt.grid_line_start_padding + without_vert_count as f64 * gap + 60.0,
            anchor: TextAnchor::Middle,
            outside: false,
        };
    }

    let mut start_x = scale.scale(task_start(task));
    let mut end_x = task
        .render_end_time
        .map(|time| scale.scale(time.epoch_milliseconds() as f64))
        .unwrap_or_else(|| scale.scale(task_end(task)));
    if task.milestone {
        start_x +=
            0.5 * (scale.scale(task_end(task)) - scale.scale(task_start(task))) - 0.5 * bar_height;
        end_x = start_x + bar_height;
    }

    let text_width = measure.width(&task.task, &Font::new("monospace", font_size));
    let y = task.order as f64 * gap + bar_height / 2.0 + (font_size / 2.0 - 2.0) + top;

    if text_width > end_x - start_x {
        if end_x + text_width + 1.5 * left > width {
            TaskLabel {
                x: start_x + left - 5.0,
                y,
                anchor: TextAnchor::End,
                outside: true,
            }
        } else {
            TaskLabel {
                x: end_x + left + 5.0,
                y,
                anchor: TextAnchor::Start,
                outside: true,
            }
        }
    } else {
        TaskLabel {
            x: (end_x - start_x) / 2.0 + start_x + left,
            y,
            anchor: TextAnchor::Middle,
            outside: false,
        }
    }
}

/// `vertLabels`: one text per category, carrying its count.
#[allow(clippy::too_many_arguments)]
fn vert_labels(
    root: &mut Group,
    category_heights: &[(String, f64)],
    gantt: &GanttConfig,
    gap: f64,
    top: f64,
    theme: &serde_json::Map<String, Value>,
    family: &str,
    measure: &dyn TextMeasure,
) {
    let mut group = Group::new().with_class("section-titles");
    let mut prev_gap = 0.0;
    for (index, (name, count)) in category_heights.iter().enumerate() {
        let y = if index > 0 {
            prev_gap += category_heights[index - 1].1;
            (count * gap) / 2.0 + prev_gap * gap + top
        } else {
            (count * gap) / 2.0 + top
        };
        let rows: Vec<&str> = name
            .split(['\r', '\n'])
            .filter(|row| !row.is_empty())
            .collect();
        let rows = if rows.is_empty() { vec![""] } else { rows };
        let dy = -((rows.len() as f64 - 1.0) / 2.0);
        let font = Font::new(family, gantt.section_font_size);
        let fill = title_color(theme);
        let mut run = text_lines_run(
            &rows,
            Point::new(10.0, y + dy * gantt.section_font_size),
            TextAnchor::Start,
            &font,
            Some(fill),
            measure,
        );
        run.line_height = gantt.section_font_size;
        group.push(run);
    }
    root.push(group);
}

/// `drawToday`: the vertical today line, unless `todayMarker off`.
#[allow(clippy::too_many_arguments)]
fn draw_today(
    root: &mut Group,
    ast: &Ast,
    gantt: &GanttConfig,
    scale: &TimeScale,
    height: f64,
    today: Date,
    theme: &serde_json::Map<String, Value>,
    left: f64,
) {
    if ast.today_marker == "off" {
        return;
    }
    let mut group = Group::new().with_class("today");
    let x = scale.scale(today.epoch_milliseconds() as f64) + left;
    let mut style = PathStyle {
        fill: None,
        stroke: theme_color(theme, "todayLineColor").map(Color::new),
        stroke_width: 2.0,
        ..PathStyle::default()
    };
    // `todayLine.attr('style', todayMarker.replace(/,/g, ';'))` overrides the CSS.
    let declarations = ast.today_marker.replace(',', ";");
    for declaration in declarations.split(';') {
        let Some((property, value)) = declaration.split_once(':') else {
            continue;
        };
        let property = property.trim();
        let value = value.trim();
        match property {
            "stroke" => style.stroke = Some(Color::new(value)),
            "stroke-width" => {
                if let Some(width) = parse_css_length(value) {
                    style.stroke_width = width;
                }
            }
            "opacity" => {
                if let Ok(opacity) = value.parse::<f64>() {
                    style.opacity = opacity;
                }
            }
            "stroke-dasharray" => {
                style.dash = value
                    .split([' ', ','])
                    .filter_map(parse_css_length)
                    .collect();
            }
            _ => {}
        }
    }

    let mut line = Path::new();
    line.move_to(Point::new(x, gantt.title_top_margin));
    line.line_to(Point::new(x, height - gantt.title_top_margin));
    group.push(PathShape::new(line, style));
    root.push(group);
}

/// A CSS length with an optional `px`, the numbers `todayMarker` styles carry.
fn parse_css_length(value: &str) -> Option<f64> {
    value
        .trim()
        .strip_suffix("px")
        .unwrap_or(value.trim())
        .parse::<f64>()
        .ok()
}

/// `styles.js`' `.section`/`.sectionN` for a row's task.
fn section_style(
    task: &Task,
    categories: &[String],
    gantt: &GanttConfig,
    theme: &serde_json::Map<String, Value>,
) -> PathStyle {
    let index = categories
        .iter()
        .position(|category| *category == task.task_type)
        .unwrap_or(0);
    let slot = (index as f64 % gantt.number_section_styles) as usize;
    let fill = match slot {
        0 => theme_color(theme, "sectionBkgColor").unwrap_or("#6666ff"),
        2 => theme_color(theme, "sectionBkgColor2").unwrap_or("#fff400"),
        _ => theme_color(theme, "altSectionBkgColor").unwrap_or("white"),
    };
    PathStyle {
        fill: Some(Color::new(fill)),
        stroke: None,
        opacity: 0.2,
        ..PathStyle::default()
    }
}

/// `styles.js`' `.task`/`.active`/`.done`/`.crit`/`.vert` cascade for a bar.
fn task_style(task: &Task, theme: &serde_json::Map<String, Value>) -> PathStyle {
    let color = |key: &str, fallback: &str| {
        theme_color(theme, key)
            .map(Color::new)
            .unwrap_or_else(|| Color::new(fallback))
    };
    let (fill, stroke) = if task.active && task.crit {
        (
            color("activeTaskBkgColor", "#bfc7ff"),
            color("critBorderColor", "#ff8888"),
        )
    } else if task.active {
        (
            color("activeTaskBkgColor", "#bfc7ff"),
            color("activeTaskBorderColor", "#534fbc"),
        )
    } else if task.done && task.crit {
        (
            color("doneTaskBkgColor", "lightgrey"),
            color("critBorderColor", "#ff8888"),
        )
    } else if task.done {
        (
            color("doneTaskBkgColor", "lightgrey"),
            color("doneTaskBorderColor", "grey"),
        )
    } else if task.crit {
        (
            color("critBkgColor", "red"),
            color("critBorderColor", "#ff8888"),
        )
    } else {
        (
            color("taskBkgColor", "#8a90dd"),
            color("taskBorderColor", "#534fbc"),
        )
    };
    let stroke = if task.vert {
        color("vertLineColor", "navy")
    } else {
        stroke
    };
    PathStyle {
        fill: Some(fill),
        stroke: Some(stroke),
        stroke_width: 2.0,
        ..PathStyle::default()
    }
}

/// `styles.js`' task text cascade: `.taskTextN` / `.taskTextOutsideN`, then the status rules
/// (`activeText`, `doneText`, done-outside, `doneCritText`), the clickable rule, and the
/// vertical marker's `!important` colour.
fn task_text_color(task: &Task, outside: bool, theme: &serde_json::Map<String, Value>) -> String {
    let theme_value =
        |key: &str, fallback: &str| theme_color(theme, key).unwrap_or(fallback).to_string();
    let mut color = if outside {
        theme_value("taskTextOutsideColor", "black")
    } else {
        theme_value("taskTextColor", "white")
    };
    let clickable = task.classes.iter().any(|class| class == "clickable");
    if clickable {
        color = theme_value("taskTextClickableColor", "#003163");
    }
    if task.active {
        color = theme_value("taskTextDarkColor", "black");
    }
    if task.done {
        color = theme_value("taskTextDarkColor", "black");
        if outside {
            color = theme_value("taskTextOutsideColor", "black");
        }
    }
    if task.done && task.crit {
        color = theme_value("taskTextDarkColor", "black");
        if outside {
            color = theme_value("taskTextOutsideColor", "black");
        }
    }
    if task.active && task.crit {
        color = theme_value("taskTextDarkColor", "black");
    }
    if task.vert {
        color = theme_value("vertLineColor", "navy");
    }
    color
}

/// The title's colour: `.titleText { fill: titleColor || textColor }`.
fn title_color(theme: &serde_json::Map<String, Value>) -> &str {
    theme_color(theme, "titleColor")
        .or_else(|| theme_color(theme, "textColor"))
        .unwrap_or("#333")
}

fn theme_color<'a>(theme: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a str> {
    theme.get(key).and_then(Value::as_str)
}

/// A one-line text run at `position`, measured like `getBBox()` in the monospace rule.
fn text_run(
    text: &str,
    position: Point,
    anchor: TextAnchor,
    font: &Font,
    fill: Option<&str>,
    measure: &dyn TextMeasure,
) -> TextRun {
    text_lines_run(&[text], position, anchor, font, fill, measure)
}

/// A run from explicit lines, the shape of a `<text>` with `<tspan>` rows.
fn text_lines_run(
    lines: &[&str],
    position: Point,
    anchor: TextAnchor,
    font: &Font,
    fill: Option<&str>,
    measure: &dyn TextMeasure,
) -> TextRun {
    let line_height = 1.1 * font.size;
    let mut block_lines = Vec::with_capacity(lines.len());
    let mut width: f64 = 0.0;
    for line in lines {
        width = width.max(measure.width(line, font));
        block_lines.push(TextLine::new(vec![TextSpan::plain(*line)]));
    }
    let height = if lines.is_empty() {
        0.0
    } else {
        measure.line_height(font) + line_height * (lines.len() as f64 - 1.0)
    };
    let block = TextBlock::new(block_lines, Size::new(width, height), line_height);
    let mut run = TextRun::new(position, font.clone(), block);
    run.anchor = anchor;
    run.fill = fill.map(Color::new);
    run
}

// ---- d3 time scale ----

/// d3-scale's `scaleTime().domain(...).rangeRound(...)`.
struct TimeScale {
    min: f64,
    max: f64,
    range: (f64, f64),
}

impl TimeScale {
    fn new(min: f64, max: f64, range0: f64, range1: f64) -> Self {
        Self {
            min,
            max,
            range: (range0, range1),
        }
    }

    fn scale(&self, value: f64) -> f64 {
        let span = self.max - self.min;
        if span == 0.0 {
            return self.range.0;
        }
        let t = (value - self.min) / span;
        js_round(self.range.0 * (1.0 - t) + self.range.1 * t)
    }
}

/// JavaScript's `Math.round`: half away from zero is wrong for negatives, JS rounds half up.
fn js_round(value: f64) -> f64 {
    if value.is_nan() || value.is_infinite() {
        value
    } else {
        (value + 0.5).floor()
    }
}

fn js_floor(value: f64) -> f64 {
    value.floor()
}

fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

/// d3-time's interval kinds.
#[derive(Clone, Copy, PartialEq, Eq)]
enum IntervalUnit {
    Millisecond,
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

/// `timeInterval.every(step)`: the base interval filtered by its field, with `weekday` as a
/// Sunday-based index for week ticks.
#[derive(Clone, Copy)]
struct Interval {
    unit: IntervalUnit,
    step: u32,
    weekday: u32,
}

impl Interval {
    fn every(unit: IntervalUnit, step: u32, weekday: u32) -> Self {
        Self {
            unit,
            step: step.max(1),
            weekday,
        }
    }

    fn floor(&self, date: Date) -> Date {
        let mut value = self.base_floor(date);
        while !self.test(value) {
            value = match Date::from_epoch_milliseconds(value.epoch_milliseconds() - 1) {
                Some(previous) => self.base_floor(previous),
                None => break,
            };
        }
        value
    }

    fn ceil(&self, date: Date) -> Date {
        let previous = Date::from_epoch_milliseconds(date.epoch_milliseconds() - 1).unwrap_or(date);
        let mut value = self.floor(previous);
        value = self.offset(value, 1);
        self.floor(value)
    }

    fn offset(&self, date: Date, steps: i64) -> Date {
        let mut value = date;
        for _ in 0..steps.max(1) {
            value = self.base_floor(self.base_offset(value, 1));
            while !self.test(value) {
                value = self.base_floor(self.base_offset(value, 1));
            }
        }
        value
    }

    fn range(&self, start: Date, stop: Date) -> Vec<Date> {
        let mut values = Vec::new();
        let mut current = self.ceil(start);
        loop {
            if current.epoch_milliseconds() >= stop.epoch_milliseconds() {
                break;
            }
            let previous = current;
            values.push(current);
            current = self.floor(self.offset(current, 1));
            if !(previous.epoch_milliseconds() < current.epoch_milliseconds()
                && current.epoch_milliseconds() < stop.epoch_milliseconds())
            {
                break;
            }
        }
        values
    }

    fn base_floor(&self, date: Date) -> Date {
        let mut date = date;
        match self.unit {
            IntervalUnit::Millisecond => date,
            IntervalUnit::Second => {
                date.millisecond = 0;
                date
            }
            IntervalUnit::Minute => {
                date.millisecond = 0;
                date.second = 0;
                date
            }
            IntervalUnit::Hour => {
                date.millisecond = 0;
                date.second = 0;
                date.minute = 0;
                date
            }
            IntervalUnit::Day => date.midnight(),
            IntervalUnit::Week => {
                let weekday = date.weekday_index();
                let back = (weekday + 7 - self.weekday) % 7;
                date.add(-(back as f64), date::Unit::Day)
                    .unwrap_or(date)
                    .midnight()
            }
            IntervalUnit::Month => {
                let mut date = date.midnight();
                date.day = 1;
                date
            }
            IntervalUnit::Year => {
                let mut date = date.midnight();
                date.month = 1;
                date.day = 1;
                date
            }
        }
    }

    fn base_offset(&self, date: Date, steps: i64) -> Date {
        match self.unit {
            IntervalUnit::Millisecond => {
                Date::from_epoch_milliseconds(date.epoch_milliseconds() + steps).unwrap_or(date)
            }
            IntervalUnit::Second => date.add(steps as f64, date::Unit::Second).unwrap_or(date),
            IntervalUnit::Minute => date.add(steps as f64, date::Unit::Minute).unwrap_or(date),
            IntervalUnit::Hour => date.add(steps as f64, date::Unit::Hour).unwrap_or(date),
            IntervalUnit::Day => date.add(steps as f64, date::Unit::Day).unwrap_or(date),
            IntervalUnit::Week => date
                .add((steps * 7) as f64, date::Unit::Day)
                .unwrap_or(date),
            IntervalUnit::Month => date.add(steps as f64, date::Unit::Month).unwrap_or(date),
            IntervalUnit::Year => date.add(steps as f64, date::Unit::Year).unwrap_or(date),
        }
    }

    fn test(&self, date: Date) -> bool {
        if self.step <= 1 {
            return true;
        }
        let step = self.step as i32;
        match self.unit {
            IntervalUnit::Millisecond => {
                // `millisecond.every(k)` floors to a multiple of k from the epoch.
                let value = date.epoch_milliseconds();
                value.rem_euclid(self.step as i64) == 0
            }
            IntervalUnit::Second => (date.second as i32) % step == 0,
            IntervalUnit::Minute => (date.minute as i32) % step == 0,
            IntervalUnit::Hour => (date.hour as i32) % step == 0,
            IntervalUnit::Day => ((date.day as i32) - 1) % step == 0,
            IntervalUnit::Week => {
                let weeks = (self.base_floor(date).epoch_milliseconds() as f64
                    - self.base_floor(from_millis(0.0)).epoch_milliseconds() as f64)
                    / DURATION_WEEK;
                (weeks.floor() as i64).rem_euclid(self.step as i64) == 0
            }
            IntervalUnit::Month => (date.month as i32 - 1) % step == 0,
            IntervalUnit::Year => (date.year as i32).rem_euclid(step) == 0,
        }
    }
}

/// d3-time-format's `timeFormat(format)` for the directives gantt axes use.
fn format_time(date: Date, format: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = format.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] != '%' {
            out.push(chars[index]);
            index += 1;
            continue;
        }
        index += 1;
        if index >= chars.len() {
            out.push('%');
            break;
        }
        let mut fill = '0';
        let mut width: Option<usize> = None;
        if chars[index] == '-' {
            fill = '\0';
            index += 1;
        } else if chars[index] == '_' {
            fill = ' ';
            index += 1;
        } else if chars[index] == '0' {
            index += 1;
        }
        let mut digits = String::new();
        while index < chars.len() && chars[index].is_ascii_digit() {
            digits.push(chars[index]);
            index += 1;
        }
        if !digits.is_empty() {
            width = digits.parse().ok();
        }
        if index >= chars.len() {
            break;
        }
        let directive = chars[index];
        index += 1;
        let text = match directive {
            '%' => "%".to_string(),
            'a' => weekday_short(date).to_string(),
            'A' => date.weekday_name().to_string(),
            'b' | 'h' => month_short(date.month).to_string(),
            'B' => month_long(date.month).to_string(),
            'd' => padded(date.day as i64, width.unwrap_or(2), fill),
            'e' => padded(date.day as i64, width.unwrap_or(2), ' '),
            'H' => padded(date.hour as i64, width.unwrap_or(2), fill),
            'I' => padded(((date.hour + 11) % 12 + 1) as i64, width.unwrap_or(2), fill),
            'j' => padded(day_of_year(date) as i64, width.unwrap_or(3), fill),
            'L' => padded(date.millisecond as i64, width.unwrap_or(3), fill),
            'm' => padded(date.month as i64, width.unwrap_or(2), fill),
            'M' => padded(date.minute as i64, width.unwrap_or(2), fill),
            'p' => {
                if date.hour < 12 {
                    "AM".to_string()
                } else {
                    "PM".to_string()
                }
            }
            'S' => padded(date.second as i64, width.unwrap_or(2), fill),
            'U' => padded(week_of_year(date, 0) as i64, width.unwrap_or(2), fill),
            'W' => padded(week_of_year(date, 1) as i64, width.unwrap_or(2), fill),
            'w' => (date.weekday_index() as i64).to_string(),
            'x' => format!(
                "{}/{}/{}",
                date.month,
                date.day,
                padded(date.year, width.unwrap_or(4), fill)
            ),
            'X' => format!(
                "{}:{}:{} {}",
                padded(((date.hour + 11) % 12 + 1) as i64, 2, '\0'),
                padded(date.minute as i64, 2, fill),
                padded(date.second as i64, 2, fill),
                if date.hour < 12 { "AM" } else { "PM" }
            ),
            'y' => padded(date.year.rem_euclid(100), width.unwrap_or(2), fill),
            'Y' => padded(date.year, width.unwrap_or(4), fill),
            'Z' => String::new(),
            other => other.to_string(),
        };
        out.push_str(&text);
    }
    out
}

fn padded(value: i64, width: usize, fill: char) -> String {
    if fill == '\0' {
        return value.to_string();
    }
    let sign = if value < 0 { "-" } else { "" };
    let digits = value.abs().to_string();
    if digits.len() >= width {
        format!("{sign}{digits}")
    } else {
        format!(
            "{sign}{}{digits}",
            fill.to_string().repeat(width - digits.len())
        )
    }
}

fn weekday_short(date: Date) -> &'static str {
    ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][date.weekday_index() as usize]
}

fn month_short(month: u32) -> &'static str {
    [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][(month.clamp(1, 12) - 1) as usize]
}

fn month_long(month: u32) -> &'static str {
    [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ][(month.clamp(1, 12) - 1) as usize]
}

fn day_of_year(date: Date) -> u32 {
    let mut days = date.day;
    for month in 1..date.month {
        days += days_in_month(date.year, month);
    }
    days
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 30,
    }
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// d3-time-format's `%U` (Sunday) and `%W` (Monday).
fn week_of_year(date: Date, start: u32) -> u32 {
    let weekday = date.weekday_index();
    let offset = (weekday + 7 - start) % 7;
    (day_of_year(date) - 1 + 7 - offset) / 7
}

/// `dayjs(maxTime).diff(dayjs(minTime), 'year')`, the >5-year guard.
fn whole_years_between(start: Date, end: Date) -> f64 {
    let months = (end.year - start.year) * 12 + end.month as i64 - start.month as i64;
    let mut whole = months / 12;
    if months % 12 != 0 && end.day < start.day {
        whole -= 1;
    }
    whole as f64
}

/// The `weekday` configuration name as a Sunday-based index.
fn weekday_index(name: &str) -> Option<u32> {
    match name {
        "sunday" => Some(0),
        "monday" => Some(1),
        "tuesday" => Some(2),
        "wednesday" => Some(3),
        "thursday" => Some(4),
        "friday" => Some(5),
        "saturday" => Some(6),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Item;

    const TOLERANCE: f64 = 0.01;

    fn fixtures() -> Vec<(String, Value)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/diagrams/gantt/tests/render");
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

    fn expected_float(value: &Value, key: &str, file: &str) -> f64 {
        value
            .get(key)
            .and_then(Value::as_f64)
            .unwrap_or_else(|| panic!("{file}: missing {key}"))
    }

    fn expected_number(value: &Value, what: &str, file: &str) -> f64 {
        value
            .as_f64()
            .unwrap_or_else(|| panic!("{file}: {what} is not a number"))
    }

    fn group<'a>(item: &'a Item, file: &str, what: &str) -> &'a Group {
        match item {
            Item::Group(group) => group,
            _ => panic!("{file}: {what} is not a group"),
        }
    }

    fn groups_with_class<'a>(root: &'a Group, class: &str) -> Vec<&'a Group> {
        root.items
            .iter()
            .filter_map(|item| match item {
                Item::Group(group) if group.class.as_deref() == Some(class) => Some(group),
                _ => None,
            })
            .collect()
    }

    fn rect_of(item: &Item) -> Option<&RectShape> {
        match item {
            Item::Rect(rect) => Some(rect),
            _ => None,
        }
    }

    fn text_of(run: &TextRun) -> String {
        run.lines
            .iter()
            .map(|line| line.text())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn fixture_today(value: &Value, file: &str) -> Date {
        let text = value["today"]["date"]
            .as_str()
            .unwrap_or_else(|| panic!("{file}: today date"));
        let (date, clock) = text.split_once('T').expect("today date and clock");
        let mut date_parts = date.split('-');
        let year = date_parts.next().expect("year").parse().expect("year");
        let month = date_parts.next().expect("month").parse().expect("month");
        let day = date_parts.next().expect("day").parse().expect("day");
        let mut clock_parts = clock.split(':');
        let hour = clock_parts.next().expect("hour").parse().expect("hour");
        let minute = clock_parts.next().expect("minute").parse().expect("minute");
        let second = clock_parts.next().expect("second").parse().expect("second");
        Date::new(year, month, day, hour, minute, second, 0)
            .unwrap_or_else(|| panic!("{file}: invalid today date"))
    }

    /// The points of a path, with `V`/`H` expanded to a line segment.
    fn path_points(path: &Path) -> Vec<Point> {
        path.commands
            .iter()
            .filter_map(|command| command.end_point())
            .collect()
    }

    /// The points of a fixture `d` string; only the `M`/`L` segments the axis domain uses.
    fn fixture_points(d: &str) -> Vec<Point> {
        let mut tokens: Vec<String> = Vec::new();
        let mut number = String::new();
        for character in d.chars() {
            if character.is_ascii_alphabetic() {
                if !number.is_empty() {
                    tokens.push(std::mem::take(&mut number));
                }
                tokens.push(character.to_string());
            } else if character == ',' || character.is_whitespace() {
                if !number.is_empty() {
                    tokens.push(std::mem::take(&mut number));
                }
            } else {
                number.push(character);
            }
        }
        if !number.is_empty() {
            tokens.push(number);
        }
        let mut points = Vec::new();
        let mut index = 0;
        let mut current = Point::ZERO;
        while index < tokens.len() {
            let op = tokens[index].as_str();
            index += 1;
            let number = |index: &mut usize| -> f64 {
                let value = tokens[*index].parse().expect("path number");
                *index += 1;
                value
            };
            match op {
                "M" | "L" => {
                    current = Point::new(number(&mut index), number(&mut index));
                    points.push(current);
                }
                "V" => {
                    current = Point::new(current.x, number(&mut index));
                    points.push(current);
                }
                "H" => {
                    current = Point::new(number(&mut index), current.y);
                    points.push(current);
                }
                other => panic!("unsupported path command {other} in {d}"),
            }
        }
        points
    }

    #[test]
    fn renders_recorded_gantt_fixtures() {
        let fixtures = fixtures();
        assert!(
            fixtures.len() >= 8,
            "expected at least 8 fixtures, found {}",
            fixtures.len()
        );

        for (file, fixture) in fixtures {
            let source = fixture["source"].as_str().expect("fixture source");
            let preprocessed = crate::preprocess::preprocess(source);
            let mut config = Config::from_source_config(&preprocessed.config)
                .unwrap_or_else(|error| panic!("{file}: config: {error}"));
            // The harness renders into a 900-unit container (`gantt.useWidth`).
            config.gantt.use_width = Some(900.0);
            let ast = super::super::parse(&preprocessed.body)
                .unwrap_or_else(|error| panic!("{file}: parse: {error}"));
            let today = fixture_today(&fixture["expected"], &file);
            let scene = draw_ast_with_today(&ast, &config, &crate::measure::FallbackMeasure, today);
            let expected = &fixture["expected"];

            let expected_view_box = &expected["viewBox"];
            close(
                scene.view_box.width,
                expected_float(expected_view_box, "width", &file),
                "viewBox width",
                &file,
            );
            close(
                scene.view_box.height,
                expected_float(expected_view_box, "height", &file),
                "viewBox height",
                &file,
            );

            let root = group(&scene.items[0], &file, "root");

            // Excluded bands.
            let expected_excludes = expected["excludeRects"].as_array().expect("excludeRects");
            let excludes = groups_with_class(root, "exclude-days");
            let actual_excludes: Vec<&RectShape> = excludes
                .first()
                .map(|group| group.items.iter().filter_map(rect_of).collect())
                .unwrap_or_default();
            assert_eq!(
                actual_excludes.len(),
                expected_excludes.len(),
                "{file}: exclude band count"
            );
            for (index, (actual, wanted)) in
                actual_excludes.iter().zip(expected_excludes).enumerate()
            {
                close(
                    actual.rect.x,
                    expected_float(wanted, "x", &file),
                    &format!("exclude {index} x"),
                    &file,
                );
                close(
                    actual.rect.y,
                    expected_float(wanted, "y", &file),
                    &format!("exclude {index} y"),
                    &file,
                );
                close(
                    actual.rect.width,
                    expected_float(wanted, "width", &file),
                    &format!("exclude {index} width"),
                    &file,
                );
                close(
                    actual.rect.height,
                    expected_float(wanted, "height", &file),
                    &format!("exclude {index} height"),
                    &file,
                );
            }

            // Grids (bottom axis, and top axis).
            let expected_grids = expected["grids"].as_array().expect("grids");
            let grids = groups_with_class(root, "grid");
            assert_eq!(grids.len(), expected_grids.len(), "{file}: grid count");
            for (grid_index, (grid, wanted)) in grids.iter().zip(expected_grids).enumerate() {
                close(
                    grid.transform.e,
                    expected_float(&wanted["transform"], "x", &file),
                    &format!("grid {grid_index} x"),
                    &file,
                );
                close(
                    grid.transform.f,
                    expected_float(&wanted["transform"], "y", &file),
                    &format!("grid {grid_index} y"),
                    &file,
                );

                let domain = match &grid.items[0] {
                    Item::Path(path) => path_points(&path.path),
                    _ => panic!("{file}: grid {grid_index} domain is not a path"),
                };
                let expected_domain = fixture_points(wanted["domain"].as_str().expect("domain"));
                assert_eq!(
                    domain.len(),
                    expected_domain.len(),
                    "{file}: grid {grid_index} domain points"
                );
                for (point_index, (actual, wanted)) in
                    domain.iter().zip(&expected_domain).enumerate()
                {
                    close(
                        actual.x,
                        wanted.x,
                        &format!("grid {grid_index} domain {point_index} x"),
                        &file,
                    );
                    close(
                        actual.y,
                        wanted.y,
                        &format!("grid {grid_index} domain {point_index} y"),
                        &file,
                    );
                }

                let expected_ticks = wanted["ticks"].as_array().expect("ticks");
                let ticks: Vec<&Group> = grid
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        Item::Group(group) if group.class.as_deref() == Some("tick") => Some(group),
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    ticks.len(),
                    expected_ticks.len(),
                    "{file}: grid {grid_index} tick count"
                );
                for (tick_index, (tick, wanted)) in ticks.iter().zip(expected_ticks).enumerate() {
                    close(
                        tick.transform.e,
                        expected_float(&wanted["transform"], "x", &file),
                        &format!("grid {grid_index} tick {tick_index} x"),
                        &file,
                    );
                    let line = match &tick.items[0] {
                        Item::Path(path) => path.path.commands[1].end_point().expect("line end"),
                        _ => panic!("{file}: tick line is not a path"),
                    };
                    close(
                        line.y,
                        expected_float(&wanted["line"], "y2", &file),
                        &format!("grid {grid_index} tick {tick_index} line y2"),
                        &file,
                    );
                    let label = match &tick.items[1] {
                        Item::Text(run) => run,
                        _ => panic!("{file}: tick label is not text"),
                    };
                    let wanted_label = &wanted["text"];
                    assert_eq!(
                        text_of(label),
                        wanted_label["text"].as_str().expect("tick text"),
                        "{file}: grid {grid_index} tick {tick_index} text"
                    );
                    close(
                        label.position.x,
                        expected_float(wanted_label, "x", &file),
                        &format!("grid {grid_index} tick {tick_index} text x"),
                        &file,
                    );
                    close(
                        label.position.y,
                        expected_number(&wanted_label["baselines"][0], "baseline", &file),
                        &format!("grid {grid_index} tick {tick_index} text baseline"),
                        &file,
                    );
                    close(
                        label.font.size,
                        expected_float(wanted_label, "fontSize", &file),
                        &format!("grid {grid_index} tick {tick_index} font size"),
                        &file,
                    );
                    close(
                        label.size.width,
                        expected_float(wanted_label, "width", &file),
                        &format!("grid {grid_index} tick {tick_index} text width"),
                        &file,
                    );
                }
            }

            // Section background rows.
            let expected_sections = expected["sections"].as_array().expect("sections");
            let sections = groups_with_class(root, "sections");
            let actual_sections: Vec<&RectShape> = sections
                .first()
                .map(|group| group.items.iter().filter_map(rect_of).collect())
                .unwrap_or_default();
            assert_eq!(
                actual_sections.len(),
                expected_sections.len(),
                "{file}: section count"
            );
            for (index, (actual, wanted)) in
                actual_sections.iter().zip(expected_sections).enumerate()
            {
                close(
                    actual.rect.x,
                    expected_float(wanted, "x", &file),
                    &format!("section {index} x"),
                    &file,
                );
                close(
                    actual.rect.y,
                    expected_float(wanted, "y", &file),
                    &format!("section {index} y"),
                    &file,
                );
                close(
                    actual.rect.width,
                    expected_float(wanted, "width", &file),
                    &format!("section {index} width"),
                    &file,
                );
                close(
                    actual.rect.height,
                    expected_float(wanted, "height", &file),
                    &format!("section {index} height"),
                    &file,
                );
            }

            // Task bars. A milestone is a group carrying the CSS transform around its rect.
            let expected_rects = expected["taskRects"].as_array().expect("taskRects");
            let tasks_group = groups_with_class(root, "tasks")
                .into_iter()
                .next()
                .unwrap_or_else(|| panic!("{file}: missing tasks group"));
            let mut actual_rects: Vec<&RectShape> = Vec::new();
            let mut milestone_transforms: Vec<Option<Transform>> = Vec::new();
            for item in &tasks_group.items {
                match item {
                    Item::Rect(rect) => {
                        actual_rects.push(rect);
                        milestone_transforms.push(None);
                    }
                    Item::Group(group) => {
                        actual_rects.push(match &group.items[0] {
                            Item::Rect(rect) => rect,
                            _ => panic!("{file}: milestone is not a rect"),
                        });
                        milestone_transforms.push(Some(group.transform));
                    }
                    _ => {}
                }
            }
            assert_eq!(
                actual_rects.len(),
                expected_rects.len(),
                "{file}: task rect count"
            );
            for (index, (actual, wanted)) in actual_rects.iter().zip(expected_rects).enumerate() {
                close(
                    actual.rect.x,
                    expected_float(wanted, "x", &file),
                    &format!("task {index} x"),
                    &file,
                );
                close(
                    actual.rect.y,
                    expected_float(wanted, "y", &file),
                    &format!("task {index} y"),
                    &file,
                );
                close(
                    actual.rect.width,
                    expected_float(wanted, "width", &file),
                    &format!("task {index} width"),
                    &file,
                );
                close(
                    actual.rect.height,
                    expected_float(wanted, "height", &file),
                    &format!("task {index} height"),
                    &file,
                );
                close(
                    actual.rx,
                    expected_float(wanted, "rx", &file),
                    &format!("task {index} rx"),
                    &file,
                );

                let class = wanted["class"].as_str().unwrap_or_default();
                if class.split(' ').any(|part| part == "milestone") {
                    let origin = wanted["transformOrigin"]
                        .as_str()
                        .unwrap_or_else(|| panic!("{file}: milestone transform origin"));
                    let (x, y) = origin
                        .split_once("px ")
                        .unwrap_or_else(|| panic!("{file}: bad transform origin {origin}"));
                    let x: f64 = x.parse().expect("origin x");
                    let y: f64 = y
                        .strip_suffix("px")
                        .expect("origin px")
                        .parse()
                        .expect("origin y");
                    let transform = milestone_transforms[index]
                        .unwrap_or_else(|| panic!("{file}: milestone {index} has no transform"));
                    let center = transform.apply(Point::new(x, y));
                    close(center.x, x, &format!("milestone {index} origin x"), &file);
                    close(center.y, y, &format!("milestone {index} origin y"), &file);
                    assert_eq!(
                        transform.a, transform.d,
                        "{file}: milestone {index} is not a rotation"
                    );
                    let angle = transform.b.atan2(transform.a).to_degrees();
                    close(angle, 45.0, &format!("milestone {index} rotation"), &file);
                    close(
                        transform.a.hypot(transform.b),
                        0.8,
                        &format!("milestone {index} scale"),
                        &file,
                    );
                }
            }

            // Task labels, in the same order as the bars.
            let expected_texts = expected["taskTexts"].as_array().expect("taskTexts");
            let actual_texts: Vec<&TextRun> = tasks_group
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Text(run) => Some(run),
                    _ => None,
                })
                .collect();
            assert_eq!(
                actual_texts.len(),
                expected_texts.len(),
                "{file}: task text count"
            );
            for (index, (actual, wanted)) in actual_texts.iter().zip(expected_texts).enumerate() {
                assert_eq!(
                    text_of(actual),
                    wanted["text"].as_str().expect("task text"),
                    "{file}: task text {index}"
                );
                close(
                    actual.position.x,
                    expected_float(wanted, "x", &file),
                    &format!("task text {index} x"),
                    &file,
                );
                close(
                    actual.position.y,
                    expected_number(&wanted["baselines"][0], "baseline", &file),
                    &format!("task text {index} y"),
                    &file,
                );
                close(
                    actual.font.size,
                    expected_float(wanted, "fontSize", &file),
                    &format!("task text {index} font size"),
                    &file,
                );
                close(
                    actual.size.width,
                    expected_float(wanted, "width", &file),
                    &format!("task text {index} width"),
                    &file,
                );
                let anchor = match wanted["anchor"].as_str().unwrap_or("start") {
                    "middle" => TextAnchor::Middle,
                    "end" => TextAnchor::End,
                    _ => TextAnchor::Start,
                };
                assert_eq!(actual.anchor, anchor, "{file}: task text {index} anchor");
                let italic = wanted["italic"].as_bool().unwrap_or(false);
                let bold = wanted["bold"].as_bool().unwrap_or(false);
                let span = &actual.lines[0].spans[0];
                assert_eq!(span.italic, italic, "{file}: task text {index} italic");
                assert_eq!(span.bold, bold, "{file}: task text {index} bold");
            }

            // Section titles.
            let expected_titles = expected["sectionTitles"].as_array().expect("sectionTitles");
            let title_groups = groups_with_class(root, "section-titles");
            let actual_titles: Vec<&TextRun> = title_groups
                .first()
                .map(|group| {
                    group
                        .items
                        .iter()
                        .filter_map(|item| match item {
                            Item::Text(run) => Some(run),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default();
            assert_eq!(
                actual_titles.len(),
                expected_titles.len(),
                "{file}: section title count"
            );
            for (index, (actual, wanted)) in actual_titles.iter().zip(expected_titles).enumerate() {
                assert_eq!(
                    text_of(actual),
                    wanted["text"].as_str().expect("section title"),
                    "{file}: section title {index}"
                );
                close(
                    actual.position.x,
                    expected_float(wanted, "x", &file),
                    &format!("section title {index} x"),
                    &file,
                );
                close(
                    actual.position.y,
                    expected_number(&wanted["baselines"][0], "baseline", &file),
                    &format!("section title {index} y"),
                    &file,
                );
                close(
                    actual.font.size,
                    expected_float(wanted, "fontSize", &file),
                    &format!("section title {index} font size"),
                    &file,
                );
                assert_eq!(
                    actual.anchor,
                    match wanted["anchor"].as_str().unwrap_or("start") {
                        "middle" => TextAnchor::Middle,
                        "end" => TextAnchor::End,
                        _ => TextAnchor::Start,
                    },
                    "{file}: section title {index} anchor"
                );
            }

            // Today line.
            match &expected["todayLine"] {
                Value::Object(_) => {
                    let today_groups = groups_with_class(root, "today");
                    let line = today_groups
                        .first()
                        .and_then(|group| match &group.items[0] {
                            Item::Path(path) => Some(path),
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("{file}: missing today line"));
                    let points = path_points(&line.path);
                    close(
                        points[0].x,
                        expected_float(&expected["todayLine"], "x1", &file),
                        "today x1",
                        &file,
                    );
                    close(
                        points[0].y,
                        expected_float(&expected["todayLine"], "y1", &file),
                        "today y1",
                        &file,
                    );
                    close(
                        points[1].x,
                        expected_float(&expected["todayLine"], "x2", &file),
                        "today x2",
                        &file,
                    );
                    close(
                        points[1].y,
                        expected_float(&expected["todayLine"], "y2", &file),
                        "today y2",
                        &file,
                    );
                    close(
                        line.style.stroke_width,
                        if expected["todayLine"]["style"].is_null() {
                            2.0
                        } else {
                            line.style.stroke_width
                        },
                        "today stroke width",
                        &file,
                    );
                }
                _ => assert!(
                    groups_with_class(root, "today").is_empty(),
                    "{file}: unexpected today line"
                ),
            }

            // Title.
            let expected_title_text = expected["title"]["text"].as_str().unwrap_or_default();
            match &expected["title"] {
                Value::Object(_) if !expected_title_text.is_empty() => {
                    let title = root
                        .items
                        .iter()
                        .rev()
                        .find_map(|item| match item {
                            Item::Text(run) => Some(run),
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("{file}: missing title"));
                    assert_eq!(
                        text_of(title),
                        expected["title"]["text"].as_str().expect("title text"),
                        "{file}: title text"
                    );
                    close(
                        title.position.x,
                        expected_float(&expected["title"], "x", &file),
                        "title x",
                        &file,
                    );
                    close(
                        title.position.y,
                        expected_float(&expected["title"], "y", &file),
                        "title y",
                        &file,
                    );
                    close(
                        title.font.size,
                        expected_float(&expected["title"], "fontSize", &file),
                        "title font size",
                        &file,
                    );
                }
                _ => {
                    let title = root.items.iter().rev().find_map(|item| match item {
                        Item::Text(run) => Some(run),
                        _ => None,
                    });
                    assert!(title.is_none(), "{file}: unexpected title text");
                }
            }
        }
    }
}

#[cfg(test)]
mod style_tests {
    use super::*;
    use crate::scene::Item;

    fn render(source: &str) -> Scene {
        let preprocessed = crate::preprocess::preprocess(source);
        let config = Config::from_source_config(&preprocessed.config).expect("config");
        let ast = super::super::parse(&preprocessed.body).expect("parse");
        draw_ast_with_today(
            &ast,
            &config,
            &crate::measure::FallbackMeasure,
            Date::new(2024, 1, 15, 12, 0, 0, 0).expect("today"),
        )
    }

    fn theme_value(config: &Config, key: &str) -> String {
        config.theme_variables[key]
            .as_str()
            .unwrap_or_else(|| panic!("{key} is not a string"))
            .to_string()
    }

    fn class_group<'a>(scene: &'a Scene, class: &str) -> Option<&'a Group> {
        let root = match &scene.items[0] {
            Item::Group(group) => group,
            _ => panic!("root is not a group"),
        };
        root.items.iter().find_map(|item| match item {
            Item::Group(group) if group.class.as_deref() == Some(class) => Some(group),
            _ => None,
        })
    }

    fn first_task(scene: &Scene) -> (&RectShape, &TextRun) {
        let group = class_group(scene, "tasks").expect("tasks group");
        let rect = group
            .items
            .iter()
            .find_map(|item| match item {
                Item::Rect(rect) => Some(rect),
                _ => None,
            })
            .expect("task rect");
        let text = group
            .items
            .iter()
            .find_map(|item| match item {
                Item::Text(text) => Some(text),
                _ => None,
            })
            .expect("task text");
        (rect, text)
    }

    fn fill_of(rect: &RectShape) -> String {
        rect.style
            .fill
            .as_ref()
            .map(|color| color.0.clone())
            .unwrap_or_default()
    }

    fn stroke_of(rect: &RectShape) -> String {
        rect.style
            .stroke
            .as_ref()
            .map(|color| color.0.clone())
            .unwrap_or_default()
    }

    #[test]
    fn paints_task_bars_and_labels_from_the_theme() {
        let cases: [(&str, &str, &str); 7] = [
            ("plain", "", "taskBkgColor"),
            ("active", "active", "activeTaskBkgColor"),
            ("done", "done", "doneTaskBkgColor"),
            ("crit", "crit", "critBkgColor"),
            ("activeCrit", "active, crit", "activeTaskBkgColor"),
            ("doneCrit", "done, crit", "doneTaskBkgColor"),
            ("vert", "vert", "taskBkgColor"),
        ];
        for (name, tags, fill_key) in cases {
            let statement = if tags.is_empty() {
                format!("  {name} :task, 2024-01-10, 3d\n")
            } else {
                format!("  {name} :{tags}, task, 2024-01-10, 3d\n")
            };
            let source = format!("gantt\n  dateFormat YYYY-MM-DD\n  section Work\n{statement}");
            let scene = render(&source);
            let (rect, text) = first_task(&scene);
            let config = Config::default();
            assert_eq!(fill_of(rect), theme_value(&config, fill_key), "{name} fill");
            let expected_stroke = if name == "vert" {
                theme_value(&config, "vertLineColor")
            } else if name == "active" {
                theme_value(&config, "activeTaskBorderColor")
            } else if name == "done" {
                theme_value(&config, "doneTaskBorderColor")
            } else if name == "crit" || name.contains("Crit") {
                theme_value(&config, "critBorderColor")
            } else {
                theme_value(&config, "taskBorderColor")
            };
            assert_eq!(stroke_of(rect), expected_stroke, "{name} stroke");
            assert_eq!(rect.style.stroke_width, 2.0, "{name} stroke width");

            let expected_text = if name == "vert" {
                theme_value(&config, "vertLineColor")
            } else if name.starts_with("active") || name.starts_with("done") {
                theme_value(&config, "taskTextDarkColor")
            } else {
                theme_value(&config, "taskTextColor")
            };
            assert_eq!(
                text.fill.as_ref().map(|color| color.0.clone()),
                Some(expected_text),
                "{name} text fill"
            );
            let expected_size = if name == "vert" { 15.0 } else { 11.0 };
            assert_eq!(text.font.size, expected_size, "{name} text size");
        }
    }

    #[test]
    fn paints_grid_sections_titles_and_excludes_from_the_theme() {
        let scene = render(
            "gantt\n  title Theme check\n  dateFormat YYYY-MM-DD\n  excludes weekends\n  section Alpha\n  A :a1, 2024-01-01, 10d\n  section Beta\n  B :b1, 2024-01-15, 3d\n",
        );
        let config = Config::default();

        let grid = class_group(&scene, "grid").expect("grid");
        let domain = match &grid.items[0] {
            Item::Path(path) => path_command_count(&path.path),
            _ => panic!("domain is not a path"),
        };
        assert_eq!(domain, 4);
        let domain_style = match &grid.items[0] {
            Item::Path(path) => &path.style,
            _ => unreachable!(),
        };
        assert_eq!(domain_style.stroke_width, 0.0);
        let tick = match &grid.items[1] {
            Item::Group(group) => group,
            _ => panic!("tick is not a group"),
        };
        let line = match &tick.items[0] {
            Item::Path(path) => &path.style,
            _ => panic!("tick line is not a path"),
        };
        assert_eq!(
            line.stroke.as_ref().map(|color| color.0.clone()),
            Some(theme_value(&config, "gridColor"))
        );
        assert_eq!(line.opacity, 0.8);
        let label = match &tick.items[1] {
            Item::Text(text) => text,
            _ => panic!("tick label is not text"),
        };
        assert_eq!(
            label.fill.as_ref().map(|color| color.0.clone()),
            Some(theme_value(&config, "textColor"))
        );
        assert_eq!(label.font.size, 10.0);

        let sections = class_group(&scene, "sections").expect("sections");
        let rects: Vec<&RectShape> = sections
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Rect(rect) => Some(rect),
                _ => None,
            })
            .collect();
        assert_eq!(fill_of(rects[0]), theme_value(&config, "sectionBkgColor"));
        assert_eq!(
            fill_of(rects[1]),
            theme_value(&config, "altSectionBkgColor")
        );
        assert_eq!(rects[0].style.opacity, 0.2);

        let titles = class_group(&scene, "section-titles").expect("titles");
        for title in &titles.items {
            if let Item::Text(run) = title {
                assert_eq!(
                    run.fill.as_ref().map(|color| color.0.clone()),
                    Some(theme_value(&config, "titleColor"))
                );
                assert_eq!(run.font.size, config.gantt.section_font_size);
                assert_eq!(run.anchor, TextAnchor::Start);
            }
        }

        let excludes = class_group(&scene, "exclude-days").expect("excludes");
        let exclude = excludes
            .items
            .iter()
            .find_map(|item| match item {
                Item::Rect(rect) => Some(rect),
                _ => None,
            })
            .expect("exclude rect");
        assert_eq!(fill_of(exclude), theme_value(&config, "excludeBkgColor"));

        let root = match &scene.items[0] {
            Item::Group(group) => group,
            _ => unreachable!(),
        };
        let title = root
            .items
            .iter()
            .rev()
            .find_map(|item| match item {
                Item::Text(run) => Some(run),
                _ => None,
            })
            .expect("title");
        assert_eq!(
            title.fill.as_ref().map(|color| color.0.clone()),
            Some(theme_value(&config, "titleColor"))
        );
        assert_eq!(title.font.size, 18.0);
    }

    fn path_command_count(path: &Path) -> usize {
        path.commands.len()
    }

    #[test]
    fn parses_the_today_marker_style_and_its_off_switch() {
        let scene = render(
            "gantt\n  dateFormat YYYY-MM-DD\n  todayMarker stroke-width:5px,stroke:#0f0,opacity:0.5\n  section Work\n  A :a1, 2024-01-01, 10d\n",
        );
        let today = class_group(&scene, "today").expect("today group");
        let line = match &today.items[0] {
            Item::Path(path) => &path.style,
            _ => panic!("today line is not a path"),
        };
        assert_eq!(line.stroke_width, 5.0);
        assert_eq!(
            line.stroke.as_ref().map(|color| color.0.clone()),
            Some("#0f0".to_string())
        );
        assert_eq!(line.opacity, 0.5);

        let scene = render(
            "gantt\n  dateFormat YYYY-MM-DD\n  todayMarker off\n  section Work\n  A :a1, 2024-01-01, 10d\n",
        );
        assert!(class_group(&scene, "today").is_none());

        let scene =
            render("gantt\n  dateFormat YYYY-MM-DD\n  section Work\n  A :a1, 2024-01-01, 10d\n");
        let today = class_group(&scene, "today").expect("today group");
        let line = match &today.items[0] {
            Item::Path(path) => &path.style,
            _ => panic!("today line is not a path"),
        };
        assert_eq!(line.stroke_width, 2.0);
        assert_eq!(
            line.stroke.as_ref().map(|color| color.0.clone()),
            Some("red".to_string())
        );
    }

    #[test]
    fn marks_a_clickable_label_bold_in_the_link_colour() {
        let source = "gantt\n  dateFormat YYYY-MM-DD\n  section Links\n  Open :doc, 2024-01-10, 3d\n  click doc href \"https://example.com/docs\"\n";
        let scene = render(source);
        let (_, text) = first_task(&scene);
        let config = Config::default();
        assert_eq!(
            text.fill.as_ref().map(|color| color.0.clone()),
            Some(theme_value(&config, "taskTextClickableColor"))
        );
        assert!(text.lines[0].spans[0].bold);
    }

    #[test]
    fn formats_axis_labels_with_d3_time_format_directives() {
        let date = Date::new(2024, 3, 7, 9, 5, 4, 30).expect("date");
        assert_eq!(format_time(date, "%Y-%m-%d"), "2024-03-07");
        assert_eq!(format_time(date, "%d/%m"), "07/03");
        assert_eq!(format_time(date, "%H:%M:%S"), "09:05:04");
        assert_eq!(format_time(date, "%b %d"), "Mar 07");
        assert_eq!(format_time(date, "%B"), "March");
        assert_eq!(format_time(date, "%a"), "Thu");
        assert_eq!(format_time(date, "%A"), "Thursday");
        assert_eq!(format_time(date, "%-d/%-m/%Y"), "7/3/2024");
        assert_eq!(format_time(date, "%I %p"), "09 AM");
        assert_eq!(format_time(date, "%j"), "067");
        assert_eq!(format_time(date, "%L"), "030");
        assert_eq!(format_time(date, "%%"), "%");
        assert_eq!(
            format_time(date, "%Y-%m-%dT%H:%M:%S"),
            "2024-03-07T09:05:04"
        );
    }

    #[test]
    fn renders_through_the_diagram_entry_point() {
        let scene = crate::render_diagram(
            "gantt\n  dateFormat YYYY-MM-DD\n  section Work\n  A :a1, 2024-01-01, 3d\n",
            &Config::default(),
        )
        .expect("render gantt");
        assert!(!scene.is_empty());
    }
}
