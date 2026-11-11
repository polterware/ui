//! The journey database after parsing: what upstream's `journeyDb.js` getters answer
//! (`getDiagramTitle`, `getAccTitle`, `getAccDescription`, `getSections`, `getTasks`, and
//! `getActors`) once the parser has run.
//!
//! Upstream keeps the task records in `rawTasks` and only copies them into `tasks` when
//! `getTasks` is called; `compileTasks` never marks a record as processed (its
//! `compileTask` reads a `processed` field that `addTask` does not set), so the records are
//! the raw ones. `getActors` reads the `tasks` array that `getTasks` fills, which is why
//! [`Ast::actors`] is `getActors` after `getTasks`; the specs call them in that order.
//!
//! `getConfig` is configuration and is not ported.

/// One task, the object `addTask` pushes and `getTasks` returns. Field names follow the
/// database's keys: upstream's `type` is [`Task::task_type`] and its `task` is
/// [`Task::task`].
#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    /// The current section when the task was added; empty before the first `section`.
    pub section: String,
    /// Upstream's `type`, a copy of [`Task::section`] made when the task is added.
    pub task_type: String,
    /// The people, split on commas and trimmed; `[]` when the data has one part, and
    /// `[""]` when its people part is empty.
    pub people: Vec<String>,
    /// The task text before the colon, as the `taskName` token spells it (no trimming).
    pub task: String,
    /// `Number(piece)` of the data's first part; `NaN` when the text is no number.
    pub score: f64,
}

/// What upstream's journey database holds after parsing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    /// `getDiagramTitle()`; empty when the diagram has no title.
    pub diagram_title: String,
    /// `getAccTitle()`; empty when the diagram has no accessible title.
    pub acc_title: String,
    /// `getAccDescription()`; empty when the diagram has no accessible description.
    pub acc_description: String,
    /// `getSections()`, in declaration order (`addSection` keeps duplicates).
    pub sections: Vec<String>,
    /// `getTasks()`, in declaration order.
    pub tasks: Vec<Task>,
    /// `getActors()` after `getTasks`: the unique people, sorted as JavaScript sorts
    /// strings (by UTF-16 code unit).
    pub actors: Vec<String>,
}
