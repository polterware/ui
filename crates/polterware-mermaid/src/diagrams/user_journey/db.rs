//! The journey database's writing side, ported from `journeyDb.js` and the accessibility
//! setters it re-exports from `diagrams/common/commonDb.ts`.
//!
//! `journeyDb.js` keeps `currentSection`, `sections`, `tasks`, and `rawTasks`; `addTask`
//! splits the `taskData` token into a score and a people list and pushes a record, and
//! `getTasks` copies `rawTasks` into `tasks` (its `compileTasks` loop reads a `processed`
//! field nothing ever sets, so it never changes a record). Here a record goes straight into
//! the database's task list and [`Db::into_ast`] answers the getters.
//!
//! `getActors` reads the `tasks` array, not `rawTasks`, so upstream answers the people only
//! after `getTasks` has run; [`Db::into_ast`] computes them from the tasks, which is the
//! order the database spec calls the two getters in.
//!
//! `sanitizeText` (DOMPurify, a DOM library) is not ported: the three common setters keep
//! the text they receive and apply only the whitespace replacements of `setAccTitle`
//! (`/^\s+/g`) and `setAccDescription` (`/\n\s+/g`). Sanitizing the title and the
//! accessibility strings belongs to the painter, as it does for the gantt and sequence
//! diagrams in this crate.

use crate::preprocess::is_js_whitespace;

use super::ast::{Ast, Task};

/// `String.prototype.trim`, JavaScript's whitespace included.
pub(super) use crate::common::js_trim;

/// The module state `journeyDb.js` keeps.
#[derive(Debug, Default)]
pub(super) struct Db {
    current_section: String,
    sections: Vec<String>,
    tasks: Vec<Task>,
    diagram_title: String,
    acc_title: String,
    acc_description: String,
}

impl Db {
    /// The cleared module state; upstream's diagram `init` calls `clear` before parsing.
    pub(super) fn new() -> Self {
        let mut db = Self::default();
        db.clear();
        db
    }

    /// `clear`: empties the collections and the common text fields.
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    /// `setDiagramTitle` (`commonDb.ts`): `sanitizeText(txt)`, without DOMPurify here.
    pub(super) fn set_diagram_title(&mut self, title: String) {
        self.diagram_title = title;
    }

    /// `setAccTitle` (`commonDb.ts`): `sanitizeText(txt).replace(/^\s+/g, '')`.
    pub(super) fn set_acc_title(&mut self, title: String) {
        self.acc_title = title.trim_start_matches(is_js_whitespace).to_string();
    }

    /// `setAccDescription` (`commonDb.ts`): `sanitizeText(txt).replace(/\n\s+/g, '\n')`.
    pub(super) fn set_acc_description(&mut self, description: String) {
        self.acc_description = collapse_indentation(&description);
    }

    /// `addSection`: makes `section` the current one and records it.
    pub(super) fn add_section(&mut self, section: String) {
        self.current_section = section.clone();
        self.sections.push(section);
    }

    /// `addTask(descr, taskData)`: `taskData.substr(1).split(':')`. The lexer always hands a
    /// `taskData` token that starts with the colon, so its first character is the one
    /// upstream drops.
    pub(super) fn add_task(&mut self, description: &str, data: &str) {
        let data = data
            .chars()
            .next()
            .map_or("", |first| &data[first.len_utf8()..]);
        let pieces: Vec<&str> = data.split(':').collect();
        let score = js_number(pieces[0]);
        let people = if pieces.len() == 1 {
            Vec::new()
        } else {
            pieces[1]
                .split(',')
                .map(|person| js_trim(person).to_string())
                .collect()
        };
        self.tasks.push(Task {
            section: self.current_section.clone(),
            task_type: self.current_section.clone(),
            people,
            task: description.to_string(),
            score,
        });
    }

    /// The getters' answers: `getTasks` and `getActors` as the specs call them, plus the
    /// common fields.
    pub(super) fn into_ast(self) -> Ast {
        let actors = actors_of(&self.tasks);
        Ast {
            diagram_title: self.diagram_title,
            acc_title: self.acc_title,
            acc_description: self.acc_description,
            sections: self.sections,
            tasks: self.tasks,
            actors,
        }
    }
}

/// `getActors`' `updateActors`: the unique people of every task, sorted the way
/// `Array.prototype.sort()` sorts strings, by UTF-16 code unit.
fn actors_of(tasks: &[Task]) -> Vec<String> {
    let mut actors: Vec<String> = Vec::new();
    for task in tasks {
        for person in &task.people {
            if !actors.contains(person) {
                actors.push(person.clone());
            }
        }
    }
    actors.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    actors
}

/// `replace(/\n\s+/g, '\n')`: each line break and the whitespace run after it become one
/// line break.
fn collapse_indentation(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        out.push(ch);
        if ch == '\n' {
            while chars.peek().is_some_and(|next| is_js_whitespace(*next)) {
                chars.next();
            }
        }
    }
    out
}

/// `Number(piece)`: JavaScript's `StringNumericLiteral`, sign, radix prefixes, `Infinity`,
/// and `NaN` included.
fn js_number(text: &str) -> f64 {
    let text = js_trim(text);
    if text.is_empty() {
        return 0.0;
    }
    if let Some(body) = text.strip_prefix('-') {
        return signed(body).map(|value| -value).unwrap_or(f64::NAN);
    }
    if let Some(body) = text.strip_prefix('+') {
        return signed(body).unwrap_or(f64::NAN);
    }
    if text == "Infinity" {
        return f64::INFINITY;
    }
    if let Some(digits) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        return radix(digits, 16).unwrap_or(f64::NAN);
    }
    if let Some(digits) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
        return radix(digits, 2).unwrap_or(f64::NAN);
    }
    if let Some(digits) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
        return radix(digits, 8).unwrap_or(f64::NAN);
    }
    decimal(text).unwrap_or(f64::NAN)
}

/// The part after an explicit sign: `Infinity` or a decimal literal (a radix prefix is not
/// allowed after a sign).
fn signed(body: &str) -> Option<f64> {
    if body == "Infinity" {
        Some(f64::INFINITY)
    } else {
        decimal(body)
    }
}

/// `StrDecimalLiteral`: digits with an optional fraction and exponent.
fn decimal(text: &str) -> Option<f64> {
    let bytes = text.as_bytes();
    let mut cursor = digits(bytes, 0);
    let before = cursor;
    let mut after = 0;
    if bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        after = digits(bytes, cursor);
        cursor += after;
    }
    if before == 0 && after == 0 {
        return None;
    }
    if matches!(bytes.get(cursor), Some(b'e' | b'E')) {
        let mark = cursor;
        cursor += 1;
        if matches!(bytes.get(cursor), Some(b'+' | b'-')) {
            cursor += 1;
        }
        let exponent = digits(bytes, cursor);
        if exponent == 0 {
            cursor = mark;
        } else {
            cursor += exponent;
        }
    }
    if cursor != bytes.len() {
        return None;
    }
    text.parse::<f64>().ok()
}

fn digits(bytes: &[u8], start: usize) -> usize {
    let mut cursor = start;
    while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    cursor - start
}

fn radix(text: &str, base: u32) -> Option<f64> {
    if text.is_empty() {
        return None;
    }
    let mut value = 0.0;
    for ch in text.chars() {
        let digit = ch.to_digit(base)?;
        value = value * f64::from(base) + f64::from(digit);
    }
    Some(value)
}
