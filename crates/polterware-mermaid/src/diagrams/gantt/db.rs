//! The database semantics of `ganttDb.js`, applied while the parser's document is reduced
//! and completed by `getTasks`' compilation pass.
//!
//! Statements mutate a [`Db`] exactly as upstream's grammar actions mutate the module-level
//! database (`setDateFormat`, `addSection`, `addTask`, `setLink`, and the rest). [`Db::get_tasks`]
//! runs `compileTasks` (at most one pass plus ten repeats) and `into_ast` turns the compiled
//! state into [`Ast`], the getters' answers.
//!
//! Differences from upstream, all forced by the environment:
//! - There is no configuration object (frontmatter and `%%{init}%%` are removed by
//!   [`crate::preprocess`]), so `getConfig().securityLevel` cannot be read: a link keeps the
//!   URL as written instead of passing it through `sanitizeUrl`, which belongs to the painter,
//!   and `setClickFun`'s early return for strict security is moot because it only registers
//!   DOM handlers.
//! - `setClickEvent`'s function name and arguments only build those DOM handlers (`funs`,
//!   `pushFun`, `bindFunctions`), which this crate does not model; the `clickable` class they
//!   add is recorded.
//! - Upstream keys `taskDb` on a plain object, so the id `__proto__` never resolves and its
//!   link and class are dropped. This port keys a map, so such a task resolves like any other.
//! - Upstream's `addTaskOrg`/`compileData` are exported but no grammar rule calls them; they
//!   are not ported.
//! - Warnings go to a list ([`Db::warnings`]) instead of the logger: unknown `after`/`until`
//!   ids, and an end that is neither a date nor a duration.
//! - Dates come from [`super::date`]: wall-clock, no zone, and "today" is today UTC.
//! - Compilation runs during parsing so that [`Ast`] equals the getters' answers. An error
//!   upstream raises when the renderer calls `getTasks` (an invalid date, an endless
//!   `excludes` loop, a task with more than three data fields, a first task with only a
//!   duration) becomes a [`ParseError`] here, carrying the task's span.

use std::collections::HashMap;

use super::ast::{Ast, Link, Task};
use super::date::{self, Date, Unit};
use crate::error::{ParseError, Span};
use crate::preprocess::is_js_whitespace;

/// JavaScript's `String.prototype.trim`: it also strips a byte-order mark, which Rust's
/// `str::trim` does not.
pub(super) fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_whitespace)
}

/// The tags `getTaskTags` strips from the front of a task's data, in its order.
const TAGS: [&str; 5] = ["active", "done", "crit", "milestone", "vert"];

/// The day a weekend starts on, in ISO weekdays (`WEEKEND_START_DAY` upstream).
fn weekend_start_day(weekend: &str) -> Option<u32> {
    match weekend {
        "friday" => Some(5),
        "saturday" => Some(6),
        _ => None,
    }
}

/// The database statements fill.
#[derive(Debug)]
pub(super) struct Db {
    date_format: String,
    axis_format: String,
    tick_interval: Option<String>,
    today_marker: String,
    includes: Vec<String>,
    excludes: Vec<String>,
    links: Vec<Link>,
    sections: Vec<String>,
    tasks: Vec<Task>,
    raw_tasks: Vec<RawTask>,
    task_db: HashMap<String, usize>,
    current_section: String,
    acc_title: String,
    acc_description: String,
    diagram_title: String,
    inclusive_end_dates: bool,
    top_axis: bool,
    weekday: String,
    weekend: String,
    last_order: i64,
    task_cnt: u32,
    last_task_id: Option<String>,
    warnings: Vec<String>,
    /// The "today" every unresolved reference and partial date falls back to.
    now: Date,
}

/// One task while it is being compiled (`rawTasks` upstream).
#[derive(Clone, Debug)]
struct RawTask {
    /// The span of the task's text, where a compilation error points.
    span: Span,
    section: String,
    processed: bool,
    manual_end_time: bool,
    render_end_time: Option<Date>,
    task: String,
    classes: Vec<String>,
    id: Option<String>,
    prev_task_id: Option<String>,
    start: Option<StartDescriptor>,
    end_data: String,
    active: bool,
    done: bool,
    crit: bool,
    milestone: bool,
    vert: bool,
    order: i64,
    start_time: Option<Date>,
    end_time: Option<Date>,
}

/// Where a task's start comes from (`raw.startTime` upstream).
#[derive(Clone, Debug)]
enum StartDescriptor {
    /// The previous task's end.
    PrevTaskEnd,
    /// A `getStartDate` call on this data.
    GetStartDate(String),
}

/// The pieces `parseData` derives from the data field.
struct TaskData {
    id: Option<String>,
    start: Option<StartDescriptor>,
    end_data: String,
    active: bool,
    done: bool,
    crit: bool,
    milestone: bool,
    vert: bool,
}

impl Db {
    pub(super) fn new() -> Self {
        Self {
            date_format: String::new(),
            axis_format: String::new(),
            tick_interval: None,
            today_marker: String::new(),
            includes: Vec::new(),
            excludes: Vec::new(),
            links: Vec::new(),
            sections: Vec::new(),
            tasks: Vec::new(),
            raw_tasks: Vec::new(),
            task_db: HashMap::new(),
            current_section: String::new(),
            acc_title: String::new(),
            acc_description: String::new(),
            diagram_title: String::new(),
            inclusive_end_dates: false,
            top_axis: false,
            weekday: "sunday".to_string(),
            weekend: "saturday".to_string(),
            last_order: 0,
            task_cnt: 0,
            last_task_id: None,
            warnings: Vec::new(),
            now: Date::today(),
        }
    }

    /// `clear`: every field back to its initial value. Only the database spec calls it.
    #[cfg(test)]
    pub(super) fn clear(&mut self) {
        *self = Self::new();
    }

    pub(super) fn set_date_format(&mut self, text: &str) {
        self.date_format = text.to_string();
    }

    pub(super) fn date_format(&self) -> &str {
        &self.date_format
    }

    pub(super) fn set_axis_format(&mut self, text: &str) {
        self.axis_format = text.to_string();
    }

    pub(super) fn axis_format(&self) -> &str {
        &self.axis_format
    }

    pub(super) fn set_tick_interval(&mut self, text: &str) {
        self.tick_interval = Some(text.to_string());
    }

    pub(super) fn tick_interval(&self) -> Option<&str> {
        self.tick_interval.as_deref()
    }

    pub(super) fn set_today_marker(&mut self, text: &str) {
        self.today_marker = text.to_string();
    }

    pub(super) fn today_marker(&self) -> &str {
        &self.today_marker
    }

    pub(super) fn set_includes(&mut self, text: &str) {
        self.includes = merge_tokens(&self.includes, text);
    }

    pub(super) fn includes(&self) -> &[String] {
        &self.includes
    }

    pub(super) fn set_excludes(&mut self, text: &str) {
        self.excludes = merge_tokens(&self.excludes, text);
    }

    pub(super) fn excludes(&self) -> &[String] {
        &self.excludes
    }

    pub(super) fn set_weekday(&mut self, text: &str) {
        self.weekday = text.to_string();
    }

    pub(super) fn weekday(&self) -> &str {
        &self.weekday
    }

    pub(super) fn set_weekend(&mut self, text: &str) {
        self.weekend = text.to_string();
    }

    pub(super) fn weekend(&self) -> &str {
        &self.weekend
    }

    pub(super) fn enable_inclusive_end_dates(&mut self) {
        self.inclusive_end_dates = true;
    }

    pub(super) fn end_dates_are_inclusive(&self) -> bool {
        self.inclusive_end_dates
    }

    pub(super) fn enable_top_axis(&mut self) {
        self.top_axis = true;
    }

    pub(super) fn top_axis_enabled(&self) -> bool {
        self.top_axis
    }

    pub(super) fn set_acc_title(&mut self, text: &str) {
        self.acc_title = text.to_string();
    }

    pub(super) fn acc_title(&self) -> &str {
        &self.acc_title
    }

    pub(super) fn set_acc_description(&mut self, text: &str) {
        self.acc_description = text.to_string();
    }

    pub(super) fn acc_description(&self) -> &str {
        &self.acc_description
    }

    pub(super) fn set_diagram_title(&mut self, text: &str) {
        self.diagram_title = text.to_string();
    }

    pub(super) fn diagram_title(&self) -> &str {
        &self.diagram_title
    }

    pub(super) fn links(&self) -> &[Link] {
        &self.links
    }

    pub(super) fn sections(&self) -> &[String] {
        &self.sections
    }

    pub(super) fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// `addSection`.
    pub(super) fn add_section(&mut self, text: &str) {
        self.current_section = text.to_string();
        self.sections.push(text.to_string());
    }

    /// `addTask`: records the task and makes it the previous task for the next statement.
    pub(super) fn add_task(&mut self, descr: &str, data: &str, span: Span) {
        let info = parse_data(&mut self.task_cnt, data);
        let order = if info.vert {
            -1
        } else {
            let order = self.last_order;
            self.last_order += 1;
            order
        };
        let index = self.raw_tasks.len();
        self.raw_tasks.push(RawTask {
            span,
            section: self.current_section.clone(),
            processed: false,
            manual_end_time: false,
            render_end_time: None,
            task: descr.to_string(),
            classes: Vec::new(),
            id: info.id.clone(),
            prev_task_id: self.last_task_id.clone(),
            start: info.start,
            end_data: info.end_data,
            active: info.active,
            done: info.done,
            crit: info.crit,
            milestone: info.milestone,
            vert: info.vert,
            order,
            start_time: None,
            end_time: None,
        });
        self.last_task_id = info.id.clone();
        // Upstream keys a plain object; an undefined id becomes the key `"undefined"`.
        self.task_db
            .insert(info.id.unwrap_or_else(|| "undefined".to_string()), index);
    }

    /// `findTaskById`, answering the task's position in the raw list.
    fn find_task_by_id(&self, id: &str) -> Option<usize> {
        self.task_db.get(id).copied()
    }

    /// `setLink`: attach a URL to each existing task id, and mark them clickable.
    pub(super) fn set_link(&mut self, ids: &str, url: &str) {
        for id in ids.split(',') {
            if self.find_task_by_id(id).is_some() {
                match self.links.iter_mut().find(|link| link.id == id) {
                    Some(link) => link.url = url.to_string(),
                    None => self.links.push(Link {
                        id: id.to_string(),
                        url: url.to_string(),
                    }),
                }
            }
        }
        self.set_class(ids, "clickable");
    }

    /// `setClass`: add a class to each existing task id.
    fn set_class(&mut self, ids: &str, class: &str) {
        for id in ids.split(',') {
            if let Some(position) = self.find_task_by_id(id) {
                self.raw_tasks[position].classes.push(class.to_string());
            }
        }
    }

    /// `setClickEvent`: upstream builds a DOM callback and marks the tasks clickable.
    pub(super) fn set_click_event(&mut self, ids: &str) {
        self.set_class(ids, "clickable");
    }

    /// `getTasks`: compile every raw task, then answer the records.
    pub(super) fn get_tasks(&mut self) -> Result<&[Task], ParseError> {
        let mut all_processed = self.compile_tasks()?;
        let mut iterations = 0;
        while !all_processed && iterations < 10 {
            all_processed = self.compile_tasks()?;
            iterations += 1;
        }
        self.tasks = self.raw_tasks.iter().map(RawTask::to_task).collect();
        Ok(&self.tasks)
    }

    /// `into_ast`: the compiled state, read through the getters.
    pub(super) fn into_ast(mut self) -> Result<Ast, ParseError> {
        self.get_tasks()?;
        Ok(Ast {
            date_format: self.date_format().to_string(),
            axis_format: self.axis_format().to_string(),
            tick_interval: self.tick_interval().map(str::to_string),
            today_marker: self.today_marker().to_string(),
            includes: self.includes().to_vec(),
            excludes: self.excludes().to_vec(),
            links: self.links().to_vec(),
            sections: self.sections().to_vec(),
            tasks: self.tasks.clone(),
            acc_title: self.acc_title().to_string(),
            acc_description: self.acc_description().to_string(),
            diagram_title: self.diagram_title().to_string(),
            inclusive_end_dates: self.end_dates_are_inclusive(),
            top_axis: self.top_axis_enabled(),
            weekday: self.weekday().to_string(),
            weekend: self.weekend().to_string(),
            warnings: self.warnings().to_vec(),
        })
    }

    /// One `compileTasks` pass; answers whether every task ended up processed.
    fn compile_tasks(&mut self) -> Result<bool, ParseError> {
        let mut all_processed = true;
        for position in 0..self.raw_tasks.len() {
            self.compile_task(position)?;
            all_processed &= self.raw_tasks[position].processed;
        }
        Ok(all_processed)
    }

    /// `compileTask`: resolve one task's start and end.
    fn compile_task(&mut self, position: usize) -> Result<(), ParseError> {
        let span = self.raw_tasks[position].span;
        let Some(descriptor) = self.raw_tasks[position].start.clone() else {
            // Upstream reads `.type` off `undefined` here, for data with more than three
            // comma-separated fields.
            return Err(ParseError::at(
                "Invalid task data: expected one to three comma-separated fields",
                span,
            ));
        };
        match descriptor {
            StartDescriptor::PrevTaskEnd => {
                let previous = match self.raw_tasks[position].prev_task_id.clone() {
                    Some(id) => self.find_task_by_id(&id),
                    None => None,
                };
                let Some(previous) = previous else {
                    // Upstream reads `.endTime` off `undefined`: the first task of a diagram
                    // cannot be given only a duration.
                    return Err(ParseError::at(
                        "Invalid task data: a task that starts after the previous task needs one",
                        span,
                    ));
                };
                let end = self.raw_tasks[previous].end_time;
                self.raw_tasks[position].start_time = end;
            }
            StartDescriptor::GetStartDate(text) => {
                let start = self.get_start_date(&text, span)?;
                if start.is_some() {
                    self.raw_tasks[position].start_time = start;
                }
            }
        }
        if let Some(start) = self.raw_tasks[position].start_time {
            let end_data = self.raw_tasks[position].end_data.clone();
            let end = self.get_end_date(start, &end_data);
            self.raw_tasks[position].end_time = end;
            if end.is_some() {
                self.raw_tasks[position].processed = true;
                self.raw_tasks[position].manual_end_time =
                    Date::parse_strict(&end_data, "YYYY-MM-DD").is_some();
                self.check_task_dates(position)?;
            }
        }
        Ok(())
    }

    /// `checkTaskDates` and `fixTaskDates`: push the end past excluded days.
    fn check_task_dates(&mut self, position: usize) -> Result<(), ParseError> {
        if self.excludes.is_empty() || self.raw_tasks[position].manual_end_time {
            return Ok(());
        }
        let Some(start) = self.raw_tasks[position].start_time else {
            return Ok(());
        };
        let Some(end) = self.raw_tasks[position].end_time else {
            return Ok(());
        };
        // `dayjs(task.startTime).add(1, 'd')`.
        let Some(start) = start.add(1.0, Unit::Day) else {
            return Ok(());
        };
        let span = self.raw_tasks[position].span;
        let (fixed_end, render_end) = fix_task_dates(
            start,
            end,
            &self.date_format,
            &self.excludes,
            &self.includes,
            &self.weekend,
            span,
        )?;
        self.raw_tasks[position].end_time = Some(fixed_end);
        self.raw_tasks[position].render_end_time = render_end;
        Ok(())
    }

    /// `getStartDate`.
    fn get_start_date(&mut self, text: &str, span: Span) -> Result<Option<Date>, ParseError> {
        let text = js_trim(text);
        if is_timestamp_format(&self.date_format)
            && !text.is_empty()
            && text.chars().all(|c| c.is_ascii_digit())
        {
            // `new Date(Number(str))`: epoch milliseconds.
            let milliseconds = text.parse::<f64>().map_or(f64::NAN, |value| value.trunc());
            return Ok(Date::from_epoch_milliseconds(milliseconds as i64));
        }
        if let Some(ids) = statement_ids(text, "after") {
            let mut latest: Option<usize> = None;
            let mut unknown = Vec::new();
            for id in ids {
                match self.find_task_by_id(id) {
                    Some(position) => {
                        let end = self.raw_tasks[position].end_time;
                        let better = match latest {
                            None => true,
                            Some(current) => js_gt(end, self.raw_tasks[current].end_time),
                        };
                        if better {
                            latest = Some(position);
                        }
                    }
                    None => unknown.push(id),
                }
            }
            if !unknown.is_empty() {
                self.warn_unknown_ids("after", &unknown);
            }
            if let Some(position) = latest {
                return Ok(self.raw_tasks[position].end_time);
            }
            return Ok(Some(self.now.midnight()));
        }
        if let Some(date) = Date::parse_strict(text, js_trim(&self.date_format)) {
            return Ok(Some(date));
        }
        match Date::parse_js(text) {
            Some(date) if (-10_000..=10_000).contains(&date.year) => Ok(Some(date)),
            _ => Err(ParseError::at(format!("Invalid date:{text}"), span)),
        }
    }

    /// `getEndDate`.
    fn get_end_date(&mut self, previous: Date, text: &str) -> Option<Date> {
        let text = js_trim(text);
        if let Some(ids) = statement_ids(text, "until") {
            let mut earliest: Option<usize> = None;
            let mut unknown = Vec::new();
            for id in ids {
                match self.find_task_by_id(id) {
                    Some(position) => {
                        let start = self.raw_tasks[position].start_time;
                        let earlier = match earliest {
                            None => true,
                            Some(current) => js_lt(start, self.raw_tasks[current].start_time),
                        };
                        if earlier {
                            earliest = Some(position);
                        }
                    }
                    None => unknown.push(id),
                }
            }
            if !unknown.is_empty() {
                self.warn_unknown_ids("until", &unknown);
            }
            if let Some(position) = earliest {
                return self.raw_tasks[position].start_time;
            }
            return Some(self.now.midnight());
        }
        if let Some(date) = Date::parse_strict(text, js_trim(&self.date_format)) {
            return Some(if self.inclusive_end_dates {
                date.add(1.0, Unit::Day).unwrap_or(date)
            } else {
                date
            });
        }
        let (value, unit) = date::parse_duration(text);
        let mut end = previous;
        if value.is_nan() {
            let format = js_trim(&self.date_format).to_string();
            self.warnings.push(format!(
                "Gantt: \"{text}\" is neither a valid date for the \"{format}\" date format nor a \
                 valid duration (e.g. \"3d\"), so it is ignored and the task gets a zero \
                 duration. Milestones need a duration too, e.g. \"Milestone :milestone, m1, \
                 2023-01-01, 0d\"."
            ));
        } else if let Some(new_end) = end.add(value, unit) {
            end = new_end;
        }
        Some(end)
    }

    /// `warnAboutUnknownTaskIds`.
    fn warn_unknown_ids(&mut self, keyword: &str, ids: &[&str]) {
        self.warnings.push(format!(
            "Gantt: the \"{keyword}\" statement references unknown task id(s): {}. Make sure \
             the referenced tasks exist and declare an id. Milestones need both an id and a \
             duration, e.g. \"Milestone :milestone, m1, 2023-01-01, 0d\".",
            ids.join(", ")
        ));
    }
}

impl RawTask {
    fn to_task(&self) -> Task {
        Task {
            id: self.id.clone().unwrap_or_default(),
            task: self.task.clone(),
            section: self.section.clone(),
            task_type: self.section.clone(),
            order: self.order,
            start_time: self.start_time,
            end_time: self.end_time,
            render_end_time: self.render_end_time,
            manual_end_time: self.manual_end_time,
            processed: self.processed,
            classes: self.classes.clone(),
            active: self.active,
            done: self.done,
            crit: self.crit,
            milestone: self.milestone,
            vert: self.vert,
            prev_task_id: self.prev_task_id.clone(),
        }
    }
}

/// JavaScript's `a > b` for two optional dates: an undefined operand makes it false.
fn js_gt(left: Option<Date>, right: Option<Date>) -> bool {
    matches!((left, right), (Some(left), Some(right)) if left > right)
}

/// JavaScript's `a < b` for two optional dates.
fn js_lt(left: Option<Date>, right: Option<Date>) -> bool {
    matches!((left, right), (Some(left), Some(right)) if left < right)
}

/// `parseData`: the id, the start descriptor, the end data, and the tags.
fn parse_data(task_cnt: &mut u32, data: &str) -> TaskData {
    let data = data.strip_prefix(':').unwrap_or(data);
    let mut fields: Vec<String> = data.split(',').map(str::to_string).collect();
    let tags = take_tags(&mut fields);
    for field in &mut fields {
        *field = js_trim(field).to_string();
    }
    let generated = |task_cnt: &mut u32| {
        *task_cnt += 1;
        format!("task{task_cnt}")
    };
    let (id, start, end_data) = match fields.len() {
        1 => (
            Some(generated(task_cnt)),
            Some(StartDescriptor::PrevTaskEnd),
            fields[0].clone(),
        ),
        2 => (
            Some(generated(task_cnt)),
            Some(StartDescriptor::GetStartDate(fields[0].clone())),
            fields[1].clone(),
        ),
        3 => (
            Some(fields[0].clone()),
            Some(StartDescriptor::GetStartDate(fields[1].clone())),
            fields[2].clone(),
        ),
        _ => (None, None, String::new()),
    };
    TaskData {
        id,
        start,
        end_data,
        active: tags.active,
        done: tags.done,
        crit: tags.crit,
        milestone: tags.milestone,
        vert: tags.vert,
    }
}

/// The flags `getTaskTags` found.
#[derive(Default)]
struct Tags {
    active: bool,
    done: bool,
    crit: bool,
    milestone: bool,
    vert: bool,
}

/// `getTaskTags`: strip leading `active`/`done`/`crit`/`milestone`/`vert` fields.
fn take_tags(fields: &mut Vec<String>) -> Tags {
    let mut tags = Tags::default();
    loop {
        let Some(first) = fields.first() else {
            break;
        };
        let matched = js_trim(first);
        let Some(tag) = TAGS.iter().position(|tag| *tag == matched) else {
            break;
        };
        match tag {
            0 => tags.active = true,
            1 => tags.done = true,
            2 => tags.crit = true,
            3 => tags.milestone = true,
            _ => tags.vert = true,
        }
        fields.remove(0);
    }
    tags
}

/// `mergeTokens`: lowercase whitespace/comma-separated tokens, appended uniquely.
fn merge_tokens(existing: &[String], text: &str) -> Vec<String> {
    let mut merged = existing.to_vec();
    let lowercased = text.to_lowercase();
    let tokens = lowercased
        .split(|c: char| is_js_whitespace(c) || c == ',')
        .filter(|token| !token.is_empty());
    for token in tokens {
        if !merged.iter().any(|existing| existing == token) {
            merged.push(token.to_string());
        }
    }
    merged
}

/// `isInvalidDate`.
pub(super) fn is_invalid_date(
    date: Date,
    date_format: &str,
    excludes: &[String],
    includes: &[String],
    weekend: &str,
) -> bool {
    let formatted = date.format(js_trim(date_format));
    let date_only = date.format("YYYY-MM-DD");
    if includes
        .iter()
        .any(|include| *include == formatted || *include == date_only)
    {
        return false;
    }
    if excludes.iter().any(|exclude| exclude == "weekends")
        && let Some(start) = weekend_start_day(weekend)
    {
        let weekday = date.iso_weekday();
        if weekday == start || weekday == start + 1 {
            return true;
        }
    }
    let weekday = date.weekday_name().to_lowercase();
    if excludes.contains(&weekday) {
        return true;
    }
    excludes
        .iter()
        .any(|exclude| *exclude == formatted || *exclude == date_only)
}

/// `fixTaskDates`: the fixed end, and the end to draw (`None` when the start is not before
/// the end).
fn fix_task_dates(
    start: Date,
    end: Date,
    date_format: &str,
    excludes: &[String],
    includes: &[String],
    weekend: &str,
    span: Span,
) -> Result<(Date, Option<Date>), ParseError> {
    let mut start = start;
    let mut end = end;
    let mut invalid = false;
    let mut render_end = None;
    let max_end = end.add(10_000.0, Unit::Day);
    while start <= end {
        if !invalid {
            render_end = Some(end);
        }
        invalid = is_invalid_date(start, date_format, excludes, includes, weekend);
        if invalid {
            let Some(next) = end.add(1.0, Unit::Day) else {
                break;
            };
            end = next;
            if max_end.is_some_and(|max_end| end > max_end) {
                return Err(ParseError::at(
                    "Failed to find a valid date that was not excluded by `excludes` after \
                     10,000 iterations.",
                    span,
                ));
            }
        }
        match start.add(1.0, Unit::Day) {
            Some(next) => start = next,
            None => break,
        }
    }
    Ok((end, render_end))
}

/// `after\s+([\d\w- ]+)` and `until\s+([\d\w- ]+)`, answering the ids.
fn statement_ids<'a>(text: &'a str, keyword: &str) -> Option<Vec<&'a str>> {
    let rest = text.strip_prefix(keyword)?;
    let whitespace = rest
        .chars()
        .take_while(|c| is_js_whitespace(*c))
        .map(char::len_utf8)
        .sum::<usize>();
    if whitespace == 0 {
        return None;
    }
    let rest = &rest[whitespace..];
    let len = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-' || *c == ' ')
        .map(char::len_utf8)
        .sum::<usize>();
    if len == 0 {
        return None;
    }
    Some(rest[..len].split(' ').filter(|id| !id.is_empty()).collect())
}

/// `isTimestampFormat`: the date format is `x` or `X`.
fn is_timestamp_format(date_format: &str) -> bool {
    let trimmed = js_trim(date_format);
    trimmed == "x" || trimmed == "X"
}
