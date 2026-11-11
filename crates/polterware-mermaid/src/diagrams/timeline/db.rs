//! The database semantics of `timelineDb.js` and the common database it forwards to
//! (`common/commonDb.ts`), applied while the parser walks the document.
//!
//! Statements mutate a [`Db`] exactly as upstream's grammar actions mutate the module-level
//! database: `setDirection` on the header, `addSection`, `addTask`, `addEvent`, and
//! `setDiagramTitle`/`setAccTitle`/`setAccDescription`. [`Db::into_ast`] turns the state
//! into [`Ast`], the getters' answers.
//!
//! Differences from upstream, all forced by the environment or dead in this grammar:
//! - `clear()` is unnecessary: [`Db::new`] starts from the same defaults the function
//!   restores.
//! - `addTaskOrg` is exported by the database but no grammar rule calls it, so it is not
//!   ported.
//! - `getTasks`' `compileTasks` loop and its `tasks.push(...rawTasks)` run once, at
//!   [`Db::into_ast`], which is what one getter call answers. Calling the getter again
//!   upstream would append the raw tasks a second time; nothing here repeats it.
//! - `sanitizeText` is a no-op: upstream runs DOMPurify, which this crate cannot, and the
//!   timeline spec never depends on it. The trims and replacements around it are kept.
//! - `addEvent`'s missing task is a JavaScript `TypeError` upstream; here [`Db::add_event`]
//!   reports it and the parser turns it into an error with the event's span.

use super::ast::{Ast, Direction, Task};
use crate::common::sanitize::collapse_description_indent;
use crate::preprocess::is_js_whitespace;

/// The state upstream's module-level timeline database keeps.
#[derive(Debug)]
pub(super) struct Db {
    direction: Direction,
    sections: Vec<String>,
    tasks: Vec<Task>,
    current_section: String,
    current_task_id: u32,
    diagram_title: String,
    acc_title: String,
    acc_description: String,
}

impl Db {
    pub(super) fn new() -> Self {
        Self {
            direction: Direction::default(),
            sections: Vec::new(),
            tasks: Vec::new(),
            current_section: String::new(),
            current_task_id: 0,
            diagram_title: String::new(),
            acc_title: String::new(),
            acc_description: String::new(),
        }
    }

    /// `setDirection`.
    pub(super) fn set_direction(&mut self, direction: Direction) {
        self.direction = direction;
    }

    /// `addSection`: the new section becomes the one later tasks belong to.
    pub(super) fn add_section(&mut self, section: String) {
        self.current_section = section.clone();
        self.sections.push(section);
    }

    /// `addTask(period, 0, '')`, the only call the grammar makes.
    pub(super) fn add_task(&mut self, period: &str) {
        self.tasks.push(Task {
            id: self.current_task_id,
            section: self.current_section.clone(),
            task_type: self.current_section.clone(),
            task: period.to_string(),
            score: 0,
            events: Vec::new(),
        });
        self.current_task_id += 1;
    }

    /// `addEvent`: appends to the task `currentTaskId - 1`, the last one declared, even when
    /// a section was opened in between. Answers `false` where upstream would throw on a
    /// diagram whose first statement is an event.
    pub(super) fn add_event(&mut self, event: String) -> bool {
        let Some(task) = self.tasks.last_mut() else {
            return false;
        };
        task.events.push(event);
        true
    }

    /// `setDiagramTitle`, which stores the text as written.
    pub(super) fn set_diagram_title(&mut self, title: String) {
        self.diagram_title = title;
    }

    /// `setAccTitle`: leading whitespace stripped.
    pub(super) fn set_acc_title(&mut self, title: String) {
        self.acc_title = title.trim_start_matches(is_js_whitespace).to_string();
    }

    /// `setAccDescription`: every line break followed by whitespace collapses to one line
    /// break (`.replace(/\n\s+/g, '\n')`).
    pub(super) fn set_acc_description(&mut self, description: String) {
        self.acc_description = collapse_description_indent(&description);
    }

    /// The getters' answers after parsing.
    pub(super) fn into_ast(self) -> Ast {
        Ast {
            direction: self.direction,
            sections: self.sections,
            tasks: self.tasks,
            diagram_title: self.diagram_title,
            acc_title: self.acc_title,
            acc_description: self.acc_description,
        }
    }
}
