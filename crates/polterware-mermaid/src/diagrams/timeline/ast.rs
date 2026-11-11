//! The timeline database after parsing: what upstream's `timelineDb.js` getters answer
//! (`getDirection`, `getSections`, `getTasks`) plus the common database's
//! `getDiagramTitle`, `getAccTitle`, and `getAccDescription`.
//!
//! `getTasks` upstream calls `compileTasks`, which never marks a task `processed`, so the
//! loop runs to its 100-iteration bound without changing anything, and the getter then
//! appends `rawTasks` to `tasks` and returns that array. One getter call therefore answers
//! the raw task records in insertion order, which is what [`Ast::tasks`] holds. `addTaskOrg`
//! is exported by the database but no grammar rule calls it, so its `description` and
//! `classes` fields never exist here.

/// The diagram's direction, upstream's `getDirection`: `LR` unless the header said `TD`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    #[default]
    Lr,
    Td,
}

impl Direction {
    /// The string `getDirection` returns.
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Lr => "LR",
            Direction::Td => "TD",
        }
    }
}

/// One entry of `getTasks()`, the record `addTask` pushes (`addEvent` fills `events`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    /// The id `addTask` numbers from zero (`currentTaskId`).
    pub id: u32,
    /// The section active when the task was declared, empty outside a section.
    pub section: String,
    /// Upstream's `type`, a copy of [`Task::section`] kept for the renderer.
    pub task_type: String,
    /// The period text, as the `period` token matched it: not trimmed, so a space before
    /// the colon stays part of the task.
    pub task: String,
    /// Upstream's `score`: the grammar always passes a length of `0`.
    pub score: u32,
    /// The events `addEvent` appended, in order, from `event` tokens without their `: `.
    pub events: Vec<String>,
}

/// The parsed diagram: what upstream's database holds after parsing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ast {
    /// `getDirection()`, `LR` by default.
    pub direction: Direction,
    /// `getSections()`, in declaration order.
    pub sections: Vec<String>,
    /// `getTasks()`.
    pub tasks: Vec<Task>,
    /// `getCommonDb().getDiagramTitle()`; empty when the diagram has no title.
    pub diagram_title: String,
    /// `getCommonDb().getAccTitle()`; empty when the diagram has no accessible title.
    pub acc_title: String,
    /// `getCommonDb().getAccDescription()`; empty when there is no description.
    pub acc_description: String,
}
