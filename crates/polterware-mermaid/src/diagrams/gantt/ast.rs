//! The gantt database after parsing: what upstream's `ganttDb.js` getters answer
//! (`getDateFormat`, `getAxisFormat`, `getTickInterval`, `getTodayMarker`, `getExcludes`,
//! `getIncludes`, `getLinks`, `getSections`, `getTasks`, the accessibility getters, the
//! title getter, `endDatesAreInclusive`, `topAxisEnabled`, `getWeekday`, and `getWeekend`).
//!
//! [`Task`] is one entry of `getTasks()`: the raw task record after `compileTasks` filled
//! `startTime`, `endTime`, `renderEndTime`, `manualEndTime`, and `processed`. Upstream also
//! keeps a `raw` field (the input string and the parsed start/end descriptors) on each
//! record; no getter exposes it, so it stays private to [`super::db`]. The renderer-only
//! parts of the database (`funs`, `bindFunctions`, `taskCnt`, `lastTask`, `lastTaskID`) are
//! not here: they bind DOM events or only feed compilation. `displayMode` is absent because
//! only configuration sets it (upstream's parser never calls `setDisplayMode`), and this
//! crate owns no configuration.

use super::date::Date;

/// The parsed diagram: what upstream's diagram database holds after parsing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    /// `getDateFormat()`.
    pub date_format: String,
    /// `getAxisFormat()`.
    pub axis_format: String,
    /// `getTickInterval()`, `None` where upstream leaves it `undefined`.
    pub tick_interval: Option<String>,
    /// `getTodayMarker()`.
    pub today_marker: String,
    /// `getIncludes()`, merged and lowercased by `setIncludes`.
    pub includes: Vec<String>,
    /// `getExcludes()`, merged and lowercased by `setExcludes`.
    pub excludes: Vec<String>,
    /// `getLinks()`: the tasks a `click ... href` statement linked, in insertion order.
    pub links: Vec<Link>,
    /// `getSections()`, in declaration order.
    pub sections: Vec<String>,
    /// `getTasks()`.
    pub tasks: Vec<Task>,
    /// `getAccTitle()`.
    pub acc_title: String,
    /// `getAccDescription()`.
    pub acc_description: String,
    /// `getDiagramTitle()`.
    pub diagram_title: String,
    /// `endDatesAreInclusive()`.
    pub inclusive_end_dates: bool,
    /// `topAxisEnabled()`.
    pub top_axis: bool,
    /// `getWeekday()`, the first day a `tickInterval` week starts on (`sunday` by default).
    pub weekday: String,
    /// `getWeekend()`: `saturday` means weekends start on Saturday, `friday` on Friday.
    pub weekend: String,
    /// What upstream sends to `log.warn` while parsing and compiling, in order: unknown
    /// `after`/`until` task ids, and an end that is neither a date nor a duration. A
    /// warning is not a getter's answer; this crate has no logger, so it keeps the list.
    pub warnings: Vec<String>,
}

/// One entry of `getLinks()`: upstream's map entry id → link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    /// The task id the URL was attached to.
    pub id: String,
    /// The URL, as written: upstream sanitizes it for display when
    /// `securityLevel !== 'loose'`, which is a painter's concern.
    pub url: String,
}

/// One entry of `getTasks()`.
#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    /// The id, an explicit first data field or the generated `task<n>`.
    pub id: String,
    /// The task text, upstream's `task` (the statement's text before the colon).
    pub task: String,
    /// The section the task was declared under, empty outside a section.
    pub section: String,
    /// Upstream's `type`, a copy of [`Task::section`].
    pub task_type: String,
    /// The serial order in the script; `-1` for a `vert` task, which takes no order.
    pub order: i64,
    /// The resolved start; `None` when compiler passes could not resolve it.
    pub start_time: Option<Date>,
    /// The resolved end; `None` while the start is unknown.
    pub end_time: Option<Date>,
    /// The end to draw when `excludes` pushed it past the last valid day; `None` when
    /// `excludes` is empty, the end is a fixed date, or the task was never adjusted.
    pub render_end_time: Option<Date>,
    /// Whether the end was a fixed `YYYY-MM-DD` date, which `checkTaskDates` leaves alone.
    pub manual_end_time: bool,
    /// Whether `compileTasks` resolved both times.
    pub processed: bool,
    /// The classes `click` statements added; only `clickable` exists in this diagram.
    pub classes: Vec<String>,
    /// The `active` tag.
    pub active: bool,
    /// The `done` tag.
    pub done: bool,
    /// The `crit` tag.
    pub crit: bool,
    /// The `milestone` tag.
    pub milestone: bool,
    /// The `vert` tag.
    pub vert: bool,
    /// The id of the task declared before this one (upstream `prevTaskId`).
    pub prev_task_id: Option<String>,
}
