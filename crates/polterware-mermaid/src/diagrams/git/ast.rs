//! The git graph database after parsing: what upstream's `gitGraphAst.ts` getters return
//! (`getCommits`, `getCommitsArray`, `getBranches`, `getBranchesAsObjArray`,
//! `getCurrentBranch`, `getDirection`, `getHead`) plus the common accessibility getters
//! (`getDiagramTitle`, `getAccTitle`, `getAccDescription`).
//!
//! Upstream keeps commits in a `Map` keyed by id and branches in a `Map` keyed by name; both
//! preserve insertion order, and `Map.set` on an existing key replaces the value in place.
//! That is a `Vec` here with [`Ast::commit`] and [`Ast::branch`] as the `get` lookups; a
//! commit whose id is reused takes the place of the first one, as `Map` does.
//!
//! `getBranchesAsObjArray` sorts the branches by their `order` with a stable sort, so equal
//! orders keep insertion order. The parser's `parseBranch` defaults a missing `order:` to
//! `0` before the database sees it, so `getBranchesAsObjArray`'s fallback for an undefined
//! order (`parseFloat('0.' + index)`) is unreachable and not modelled.
//!
//! Left out because this crate owns no configuration: `getConfig`, and `getOptions`/
//! `setOptions` (the `gitGraph` options object is only filled by a directive, which
//! preprocessing removes before the parser runs). `prettyPrint` only logs and `clear` only
//! resets the state built by [`super::parse`]. The root branch is therefore `main` with
//! `order` `0`, the defaults of `mainBranchName` and `mainBranchOrder`.

/// A diagram direction (`DiagramOrientation` in `gitGraphTypes.ts`); `LR` until the header
/// says otherwise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    #[default]
    Lr,
    Tb,
    Bt,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Lr => "LR",
            Direction::Tb => "TB",
            Direction::Bt => "BT",
        }
    }
}

/// A commit's `type` (`commitType` in `gitGraphTypes.ts`), in upstream's numeric order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommitType {
    #[default]
    Normal,
    Reverse,
    Highlight,
    Merge,
    CherryPick,
}

impl CommitType {
    /// The number upstream stores in `Commit.type` and `Commit.customType`.
    pub fn as_number(self) -> u8 {
        match self {
            CommitType::Normal => 0,
            CommitType::Reverse => 1,
            CommitType::Highlight => 2,
            CommitType::Merge => 3,
            CommitType::CherryPick => 4,
        }
    }
}

/// One commit (`Commit` in `gitGraphTypes.ts`), exactly the fields the database stores.
#[derive(Clone, Debug, PartialEq)]
pub struct Commit {
    /// `Commit.id`: the custom id, or `"{seq}-{seven hex}"` when the source gave none.
    pub id: String,
    /// `Commit.message`; empty when the commit has no message.
    pub message: String,
    /// `Commit.seq`: the creation order, from zero.
    pub seq: u32,
    /// `Commit.type`: `Normal`, `Reverse`, or `Highlight` for `commit`, `Merge` for a merge,
    /// and `CherryPick` for a cherry-pick.
    pub kind: CommitType,
    /// `Commit.tags`; empty when the commit has none. Tags are the decoded strings, not the
    /// quotes around them.
    pub tags: Vec<String>,
    /// `Commit.parents`: the previous head's id, plus the merged branch's head for a merge,
    /// plus the cherry-picked commit's id for a cherry-pick.
    pub parents: Vec<String>,
    /// `Commit.branch`: the branch that was checked out when the commit was created.
    pub branch: String,
    /// `Commit.customType`: the `type:` override a merge was given, if any.
    pub custom_type: Option<CommitType>,
    /// `Commit.customId`: whether a merge was given an explicit id.
    pub custom_id: bool,
}

/// One branch (`getBranches` entry plus its `branchConfig` order).
#[derive(Clone, Debug, PartialEq)]
pub struct Branch {
    /// The branch name, sanitized.
    pub name: String,
    /// The branch's head commit id; `None` for a branch created before any commit.
    pub head: Option<String>,
    /// The `order:` from `branch`, `0` when absent (as `parseBranch` coerces it).
    pub order: i64,
}

/// What upstream's git graph database holds after parsing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    /// `getCommits()`, in insertion order.
    pub commits: Vec<Commit>,
    /// `getBranches()`, in insertion order (`main` first).
    pub branches: Vec<Branch>,
    /// `getCurrentBranch()`.
    pub current_branch: String,
    /// `getDirection()`.
    pub direction: Direction,
    /// The id of the commit `getHead()` returns; `None` before the branch's first commit.
    pub head: Option<String>,
    /// `getDiagramTitle()`; empty when the diagram has no title.
    pub diagram_title: String,
    /// `getAccTitle()`; empty when the diagram has no accessible title.
    pub acc_title: String,
    /// `getAccDescription()`; empty when the diagram has no accessible description.
    pub acc_description: String,
}

impl Ast {
    /// The commit with this id (`getCommits().get(id)`). A reused id keeps the first
    /// insertion position and the last value, as a `Map` does.
    pub fn commit(&self, id: &str) -> Option<&Commit> {
        self.commits.iter().find(|commit| commit.id == id)
    }

    /// `getCommitsArray()`: every commit sorted by `seq`.
    pub fn commits_by_seq(&self) -> Vec<&Commit> {
        let mut commits: Vec<&Commit> = self.commits.iter().collect();
        commits.sort_by_key(|commit| commit.seq);
        commits
    }

    /// The branch with this name (`getBranches().get(name)`).
    pub fn branch(&self, name: &str) -> Option<&Branch> {
        self.branches.iter().find(|branch| branch.name == name)
    }

    /// `getBranchesAsObjArray()`: the branch names sorted by `order`, stable on ties.
    pub fn branches_as_obj_array(&self) -> Vec<&str> {
        let mut branches: Vec<&Branch> = self.branches.iter().collect();
        branches.sort_by_key(|branch| branch.order);
        branches
            .into_iter()
            .map(|branch| branch.name.as_str())
            .collect()
    }

    /// `getHead()`: the commit the current branch points at.
    pub fn head(&self) -> Option<&Commit> {
        self.head.as_deref().and_then(|id| self.commit(id))
    }
}
