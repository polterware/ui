//! The git graph database's writing side, ported from `gitGraphAst.ts` and
//! `diagrams/common/populateCommonDb.ts`.
//!
//! `gitGraphParser.ts` parses the whole document first and then populates the database
//! (`populateCommonDb`, then `setDirection`, then each statement in source order), so every
//! database error is raised after the grammar accepted the whole body. The error messages are
//! upstream's, character for character, and the checks run in upstream's order because more
//! than one of them can apply to the same `merge`.
//!
//! `sanitizeText` runs over every id, message, tag, and branch name that reaches the database
//! (see [`super::sanitize`]). Commit ids upstream did not receive are drawn as
//! `"{seq}-{seven hex}"` from `Math.random`; a counter supplies the hex digits here, so the
//! port keeps the shape and a parse stays reproducible.

use super::ast::{Ast, Branch, Commit, CommitType, Direction};
use super::parser::{
    BranchStatement, CheckoutStatement, CherryPickStatement, CommitStatement, MergeStatement,
    ParsedGitGraph, Statement,
};
use super::sanitize;
use crate::error::{ParseError, Span};

/// `populate`: applies the parsed document to a fresh database.
pub(crate) fn build(parsed: ParsedGitGraph) -> Result<Ast, ParseError> {
    let mut db = Db::new();
    if let Some(value) = parsed.acc_description.filter(|value| !value.is_empty()) {
        db.set_acc_description(value);
    }
    if let Some(value) = parsed.acc_title.filter(|value| !value.is_empty()) {
        db.set_acc_title(value);
    }
    if let Some(value) = parsed.title.filter(|value| !value.is_empty()) {
        db.set_diagram_title(value);
    }
    if let Some(direction) = parsed.direction {
        db.direction = direction;
    }
    for statement in parsed.statements {
        match statement {
            Statement::Commit(statement) => db.commit(statement),
            Statement::Branch(statement) => db.branch(statement)?,
            Statement::Merge(statement) => db.merge(statement)?,
            Statement::Checkout(statement) => db.checkout_statement(statement)?,
            Statement::CherryPick(statement) => db.cherry_pick(statement)?,
        }
    }
    Ok(db.into_ast())
}

/// A branch as `getBranches` holds it: the name and the head commit id.
struct BranchState {
    name: String,
    head: Option<String>,
}

/// The module state `gitGraphAst.ts` keeps.
struct Db {
    commits: Vec<Commit>,
    head: Option<String>,
    branches: Vec<BranchState>,
    /// `branchConfig`: the `order:` each branch was created with, in creation order.
    branch_config: Vec<(String, i64)>,
    current_branch: String,
    direction: Direction,
    seq: u32,
    next_random_id: u32,
    diagram_title: String,
    acc_title: String,
    acc_description: String,
}

impl Db {
    fn new() -> Self {
        let main_branch = "main".to_string();
        Self {
            commits: Vec::new(),
            head: None,
            branches: vec![BranchState {
                name: main_branch.clone(),
                head: None,
            }],
            branch_config: vec![(main_branch.clone(), 0)],
            current_branch: main_branch,
            direction: Direction::Lr,
            seq: 0,
            next_random_id: 0,
            diagram_title: String::new(),
            acc_title: String::new(),
            acc_description: String::new(),
        }
    }

    /// `setDiagramTitle`: `sanitizeText(txt)`.
    fn set_diagram_title(&mut self, title: String) {
        self.diagram_title = sanitize::sanitize_text(&title);
    }

    /// `setAccTitle`: `sanitizeText(txt).replace(/^\s+/g, '')`.
    fn set_acc_title(&mut self, title: String) {
        self.acc_title = sanitize::set_acc_title(&title);
    }

    /// `setAccDescription`: `sanitizeText(txt).replace(/\n\s+/g, '\n')`.
    fn set_acc_description(&mut self, description: String) {
        self.acc_description = sanitize::set_acc_description(&description);
    }

    fn commit_of(&self, id: &str) -> Option<&Commit> {
        self.commits.iter().find(|commit| commit.id == id)
    }

    /// `commits.set(id, commit)`: a reused id keeps its first position and takes the new value.
    fn set_commit(&mut self, commit: Commit) {
        match self
            .commits
            .iter_mut()
            .find(|existing| existing.id == commit.id)
        {
            Some(existing) => *existing = commit,
            None => self.commits.push(commit),
        }
    }

    /// `branches.get(name)`: `None` when the branch does not exist, otherwise its head.
    fn branch_head(&self, name: &str) -> Option<Option<String>> {
        self.branches
            .iter()
            .find(|branch| branch.name == name)
            .map(|branch| branch.head.clone())
    }

    fn set_branch_head(&mut self, name: &str, head: String) {
        if let Some(branch) = self.branches.iter_mut().find(|branch| branch.name == name) {
            branch.head = Some(head);
        }
    }

    /// `${state.records.seq}-${random({ length: 7 })}`, with a counter in place of `Math.random`.
    fn auto_id(&mut self) -> String {
        let suffix = format!("{:07x}", self.next_random_id);
        self.next_random_id += 1;
        format!("{}-{suffix}", self.seq)
    }

    /// `commit`: appends a commit on the current branch and moves its head.
    fn commit(&mut self, statement: CommitStatement) {
        let id = statement
            .id
            .as_deref()
            .map(sanitize::sanitize_text)
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| self.auto_id());
        let message = sanitize::sanitize_text(statement.message.as_deref().unwrap_or(""));
        let tags = statement
            .tags
            .iter()
            .map(|tag| sanitize::sanitize_text(tag))
            .collect();
        let commit = Commit {
            id: id.clone(),
            message,
            seq: self.seq,
            kind: statement.kind.unwrap_or(CommitType::Normal),
            tags,
            parents: self.head.iter().cloned().collect(),
            branch: self.current_branch.clone(),
            custom_type: None,
            custom_id: false,
        };
        self.seq += 1;
        self.head = Some(id.clone());
        self.set_commit(commit);
        let current_branch = self.current_branch.clone();
        self.set_branch_head(&current_branch, id);
    }

    /// `branch`: refuses an existing name, records the branch at the current head, then
    /// checks it out.
    fn branch(&mut self, statement: BranchStatement) -> Result<(), ParseError> {
        let name = sanitize::sanitize_text(&statement.name);
        if self.branch_head(&name).is_some() {
            return Err(ParseError::at(
                format!(
                    "Trying to create an existing branch. (Help: Either use a new name if you want create a new branch or try using \"checkout {name}\")"
                ),
                statement.span,
            ));
        }
        let head = self.head.clone();
        self.branches.push(BranchState {
            name: name.clone(),
            head,
        });
        self.branch_config
            .push((name.clone(), statement.order.unwrap_or(0)));
        self.checkout(&name, statement.span)
    }

    /// `merge`: the checks run in upstream's order, then a merge commit joins the two heads.
    fn merge(&mut self, statement: MergeStatement) -> Result<(), ParseError> {
        let other_branch = sanitize::sanitize_text(&statement.branch);
        let custom_id = statement
            .id
            .as_deref()
            .map(sanitize::sanitize_text)
            .filter(|id| !id.is_empty());
        let current_head = self.branch_head(&self.current_branch).flatten();
        let other_head = self.branch_head(&other_branch).flatten();
        let current_commit = current_head.as_deref().and_then(|id| self.commit_of(id));
        let other_commit = other_head.as_deref().and_then(|id| self.commit_of(id));

        if let (Some(current), Some(_)) = (current_commit, other_commit)
            && current.branch == other_branch
        {
            return Err(ParseError::at(
                format!("Cannot merge branch '{other_branch}' into itself."),
                statement.span,
            ));
        }
        if self.current_branch == other_branch {
            return Err(ParseError::at(
                "Incorrect usage of \"merge\". Cannot merge a branch to itself",
                statement.span,
            ));
        }
        if current_commit.is_none() {
            return Err(ParseError::at(
                format!(
                    "Incorrect usage of \"merge\". Current branch ({})has no commits",
                    self.current_branch
                ),
                statement.span,
            ));
        }
        if self.branch_head(&other_branch).is_none() {
            return Err(ParseError::at(
                format!(
                    "Incorrect usage of \"merge\". Branch to be merged ({other_branch}) does not exist"
                ),
                statement.span,
            ));
        }
        if other_commit.is_none() {
            return Err(ParseError::at(
                format!(
                    "Incorrect usage of \"merge\". Branch to be merged ({other_branch}) has no commits"
                ),
                statement.span,
            ));
        }
        if current_commit.map(|commit| commit.id.as_str())
            == other_commit.map(|commit| commit.id.as_str())
        {
            return Err(ParseError::at(
                "Incorrect usage of \"merge\". Both branches have same head",
                statement.span,
            ));
        }
        if let Some(custom_id) = &custom_id
            && self.commit_of(custom_id).is_some()
        {
            return Err(ParseError::at(
                format!(
                    "Incorrect usage of \"merge\". Commit with id:{custom_id} already exists, use different custom id"
                ),
                statement.span,
            ));
        }

        let id = custom_id.clone().unwrap_or_else(|| self.auto_id());
        let parents = match &self.head {
            None => Vec::new(),
            Some(head) => vec![head.clone(), other_head.clone().unwrap_or_default()],
        };
        let commit = Commit {
            id: id.clone(),
            message: format!("merged branch {other_branch} into {}", self.current_branch),
            seq: self.seq,
            kind: CommitType::Merge,
            tags: statement
                .tags
                .iter()
                .map(|tag| sanitize::sanitize_text(tag))
                .collect(),
            parents,
            branch: self.current_branch.clone(),
            custom_type: statement.kind,
            custom_id: custom_id.is_some(),
        };
        self.seq += 1;
        self.head = Some(id.clone());
        self.set_commit(commit);
        let current_branch = self.current_branch.clone();
        self.set_branch_head(&current_branch, id);
        Ok(())
    }

    /// `cherryPick`: copies another branch's commit onto the current branch. The parser always
    /// answers an empty `targetId`, so the "cherry-pick onto the current branch" path is the
    /// only one upstream can take.
    fn cherry_pick(&mut self, statement: CherryPickStatement) -> Result<(), ParseError> {
        let source_id = statement.id.as_deref().map(sanitize::sanitize_text);
        let tags = statement.tags.as_ref().map(|tags| {
            tags.iter()
                .map(|tag| sanitize::sanitize_text(tag))
                .collect::<Vec<_>>()
        });
        let parent = statement
            .parent
            .as_deref()
            .map(sanitize::sanitize_text)
            .filter(|parent| !parent.is_empty());

        let Some(source) = source_id
            .filter(|id| !id.is_empty())
            .and_then(|id| self.commit_of(&id))
            .cloned()
        else {
            return Err(ParseError::at(
                "Incorrect usage of \"cherryPick\". Source commit id should exist and provided",
                statement.span,
            ));
        };
        if let Some(parent) = &parent
            && !source.parents.iter().any(|candidate| candidate == parent)
        {
            return Err(ParseError::at(
                "Invalid operation: The specified parent commit is not an immediate parent of the cherry-picked commit.",
                statement.span,
            ));
        }
        if source.kind == CommitType::Merge && parent.is_none() {
            return Err(ParseError::at(
                "Incorrect usage of cherry-pick: If the source commit is a merge commit, an immediate parent commit must be specified.",
                statement.span,
            ));
        }
        if source.branch == self.current_branch {
            return Err(ParseError::at(
                "Incorrect usage of \"cherryPick\". Source commit is already on current branch",
                statement.span,
            ));
        }
        let current_head = self.branch_head(&self.current_branch).flatten();
        let has_current_commit = current_head
            .filter(|id| !id.is_empty())
            .is_some_and(|id| self.commit_of(&id).is_some());
        if !has_current_commit {
            return Err(ParseError::at(
                format!(
                    "Incorrect usage of \"cherry-pick\". Current branch ({})has no commits",
                    self.current_branch
                ),
                statement.span,
            ));
        }

        let message = format!(
            "cherry-picked {} into {}",
            source.message, self.current_branch
        );
        let parents = match &self.head {
            None => Vec::new(),
            Some(head) => vec![head.clone(), source.id.clone()],
        };
        let tags = match tags {
            Some(tags) => tags.into_iter().filter(|tag| !tag.is_empty()).collect(),
            None => {
                let parent = if source.kind == CommitType::Merge {
                    format!("|parent:{}", parent.unwrap_or_default())
                } else {
                    String::new()
                };
                vec![format!("cherry-pick:{}{parent}", source.id)]
            }
        };
        let id = self.auto_id();
        let commit = Commit {
            id: id.clone(),
            message,
            seq: self.seq,
            kind: CommitType::CherryPick,
            tags,
            parents,
            branch: self.current_branch.clone(),
            custom_type: None,
            custom_id: false,
        };
        self.seq += 1;
        self.head = Some(id.clone());
        self.set_commit(commit);
        let current_branch = self.current_branch.clone();
        self.set_branch_head(&current_branch, id);
        Ok(())
    }

    fn checkout_statement(&mut self, statement: CheckoutStatement) -> Result<(), ParseError> {
        self.checkout(&statement.branch, statement.span)
    }

    /// `checkout`: moves the current branch and its head, or refuses an unknown branch.
    fn checkout(&mut self, branch: &str, span: Span) -> Result<(), ParseError> {
        let branch = sanitize::sanitize_text(branch);
        let Some(head) = self.branch_head(&branch) else {
            return Err(ParseError::at(
                format!(
                    "Trying to checkout branch which is not yet created. (Help try using \"branch {branch}\")"
                ),
                span,
            ));
        };
        self.current_branch = branch;
        let head = head.filter(|id| !id.is_empty());
        let head = head.filter(|id| self.commit_of(id).is_some());
        self.head = head;
        Ok(())
    }

    fn into_ast(self) -> Ast {
        let branches = self
            .branch_config
            .iter()
            .map(|(name, order)| Branch {
                name: name.clone(),
                head: self
                    .branch_head(name)
                    .flatten()
                    .filter(|head| !head.is_empty()),
                order: *order,
            })
            .collect();
        Ast {
            commits: self.commits,
            branches,
            current_branch: self.current_branch,
            direction: self.direction,
            head: self.head,
            diagram_title: self.diagram_title,
            acc_title: self.acc_title,
            acc_description: self.acc_description,
        }
    }
}
