//! Source-to-[`Scene`] rendering for git graphs: the Rust side of `gitGraphRenderer.ts`
//! `draw`.
//!
//! Look: `diagrams/git/styles.js` (`.branch`, `.branch-label`, `.commit-label`,
//! `.commit-label-bkg`, `.tag-label`, `.tag-label-bkg`, `.tag-hole`, `.commitN`, `.labelN`,
//! `.arrowN`, `.gitTitleText`) and the attributes `gitGraphRenderer.ts` writes. The geometry is
//! upstream's: `LAYOUT_OFFSET`, `COMMIT_STEP`, `PX`, `PY`, the branch positions from
//! `setBranchPosition`, the commit positions from `drawCommits`, the lane search
//! (`findLane`), the arrow path shapes, the bullet shapes, the rotated commit labels and the
//! tag polygons.
//!
//! Behavior: `gitGraphRenderer.ts` draws with plain d3 and has no gpui-kit counterpart, so the
//! renderer itself is the model, followed call by call: the two `drawCommits` passes (the first
//! only assigns positions), `drawBranches`, `drawArrows`, `drawCommitBullet`,
//! `drawCommitLabel`, `drawCommitTags`, `insertTitle`, and `setupGraphViewbox`.
//!
//! Themes: the default (classic) palette is ported from `styles.js`; the redux geometry themes
//! (`redux`, `redux-dark`, `redux-color`, `redux-dark-color`) change the bullet sizes, the
//! label radii and the spine offset, and the neo colour themes resolve their branch label
//! colours differently. The theme's branch colours (`git0`..`git7`), `gitInv*`,
//! `gitBranchLabel*`, `nodeBorder`, `primaryColor`, `lineColor`, `textColor`,
//! `commitLabel*`, `tagLabel*` and the font sizes come from
//! [`Config::theme_variables`](crate::config::Config::theme_variables).
//!
//! Text is measured in the `monospace` family, the rule the fixture harness renders with and
//! that [`FallbackMeasure`](crate::measure::FallbackMeasure) reproduces; a browser's theme
//! family would measure differently. A [`TextRun`] carries the first line's baseline, so the
//! `dy="1em"` of `drawText`'s `<tspan>` rows is the run's `y` and the painter draws the same
//! glyphs.
//!
//! Where the port stops short of upstream, the difference is in the painter-facing scene:
//!
//! * CSS classes only select; a [`Scene`] carries paints, so `styles.js`'s declarations are
//!   resolved into the shapes (the cascade included: the later `.labelN` rule wins over
//!   `.branch-labelN`, so a branch's label text and its background share the branch colour).
//! * The `neo` look's drop shadow, the `useGradient` stroke on branch labels and
//!   `mix-blend-mode` are CSS effects with no [`PathStyle`] equivalent; the shapes keep their
//!   flat fill.
//! * `setupGraphViewbox` frames the diagram from the SVG's `getBBox()`. The scene's
//!   [`Scene::view_box`] is the union of the items it paints, grown by `diagramPadding` with
//!   the same rule the fixture harness measures (a text box from its baseline minus its ink
//!   height, arcs sampled). A browser's font metrics differ; the painted coordinates do not.
//! * A parent id with no commit (impossible through the database) would make upstream throw;
//!   the scene skips that arrow.

use std::collections::HashMap;

use serde_json::Value;

use crate::config::{Config, GitGraphConfig};
use crate::error::ParseError;
use crate::measure::{Font, TextMeasure, TextOptions, shape_text};
use crate::render::shapes::label_image_utils::parse_font_size;
use crate::scene::{
    Color, EllipseShape, Group, Item, LineCap, Path, PathCommand, PathShape, PathStyle, Point,
    Rect, RectShape, Scene, TextAnchor, TextRun, Transform,
};

use super::Ast;
use super::ast::{Commit, CommitType, Direction};

/// `LAYOUT_OFFSET`: the gap between a commit's position and its bullet.
const LAYOUT_OFFSET: f64 = 10.0;
/// `COMMIT_STEP`: the distance between two commits along the diagram's axis.
const COMMIT_STEP: f64 = 40.0;
/// `PX`: the horizontal tag padding.
const PX: f64 = 4.0;
/// `PY`: the vertical tag and commit label padding.
const PY: f64 = 2.0;
/// `THEME_COLOR_LIMIT`: the number of `git0`..`git7` colour rules `styles.js` writes.
const THEME_COLOR_LIMIT: usize = 8;
/// `REDUX_BRANCH_LABEL_PADDING_Y`.
const REDUX_BRANCH_LABEL_PADDING_Y: f64 = 12.0;
/// `defaultPos`, the first commit position along the vertical axis.
const DEFAULT_POS: f64 = 30.0;
/// `drawText`'s line advance, `1em` of the branch label font.
const LINE_DY_EM: f64 = 1.0;
/// `commitLabelFontSize`'s fallback, the `10px` of `styles.js`.
const COMMIT_LABEL_FONT_SIZE: f64 = 10.0;
/// `tagLabelFontSize`'s fallback.
const TAG_LABEL_FONT_SIZE: f64 = 10.0;
/// `.gitTitleText`'s `font-size: 18px`.
const TITLE_FONT_SIZE: f64 = 18.0;
/// The theme font size, `styles.js`'s inherited `font-size`.
const BRANCH_FONT_SIZE: f64 = 16.0;
/// d3's `epsilon`.
const EPSILON: f64 = 1e-6;

/// Renders a git graph body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(body: &str, config: &Config, measure: &dyn TextMeasure) -> Result<Scene, ParseError> {
    let ast = super::parse(body)?;
    Ok(draw_ast(&ast, config, measure))
}

/// Renders an already parsed git graph database.
pub fn draw_ast(ast: &Ast, config: &Config, measure: &dyn TextMeasure) -> Scene {
    GitRender::new(ast, config, measure).draw()
}

/// Renders a git graph body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, ParseError> {
    draw(body, config, &crate::measure::FallbackMeasure)
}

/// `getBranchesAsObjArray` order's per-branch state: the position along the perpendicular
/// axis and the index `styles.js` colours by.
#[derive(Clone, Copy, Debug)]
struct BranchPosition {
    pos: f64,
    index: usize,
}

/// A commit's position, as the two `drawCommits` passes store it in `commitPos`.
#[derive(Clone, Copy, Debug)]
struct CommitPosition {
    x: f64,
    y: f64,
}

/// `getCommitPosition`'s return value.
#[derive(Clone, Copy, Debug)]
struct CommitPositionOffset {
    x: f64,
    y: f64,
    pos_with_offset: f64,
}

/// The renderer's port of the module state and `draw`.
struct GitRender<'a> {
    ast: &'a Ast,
    commits: Vec<Commit>,
    git: &'a GitGraphConfig,
    theme: &'a serde_json::Map<String, Value>,
    measure: &'a dyn TextMeasure,
    dir: Direction,
    /// `branchPos`.
    branch_pos: HashMap<String, BranchPosition>,
    /// `commitPos`.
    commit_pos: HashMap<String, CommitPosition>,
    /// `lanes`, the free horizontal/vertical lanes `findLane` hands out.
    lanes: Vec<f64>,
    /// `maxPos`.
    max_pos: f64,
    use_redux_geometry: bool,
    use_color_theme: bool,
    is_dark: bool,
    neo_color_gen: bool,
    branch_font: Font,
    commit_label_font: Font,
    tag_font: Font,
    title_font: Font,
    branch_items: Vec<Item>,
    arrow_items: Vec<Item>,
    bullet_items: Vec<Item>,
    label_items: Vec<Item>,
}

impl<'a> GitRender<'a> {
    fn new(ast: &'a Ast, config: &'a Config, measure: &'a dyn TextMeasure) -> Self {
        let theme = &config.theme_variables;
        let theme_name = config.theme.as_str();
        let redux = matches!(
            theme_name,
            "redux" | "redux-dark" | "redux-color" | "redux-dark-color"
        );
        let color_theme = matches!(theme_name, "redux-color" | "redux-dark-color");
        let dark = matches!(
            theme_name,
            "dark" | "redux-dark" | "redux-dark-color" | "neo-dark"
        );
        let neo = matches!(
            theme_name,
            "redux" | "redux-dark" | "redux-color" | "redux-dark-color" | "neo" | "neo-dark"
        );
        let font_size = theme
            .get("fontSize")
            .and_then(Value::as_str)
            .and_then(parse_font_size)
            .unwrap_or(BRANCH_FONT_SIZE);
        let number = |key: &str, fallback: f64| {
            theme
                .get(key)
                .and_then(Value::as_str)
                .and_then(parse_font_size)
                .unwrap_or(fallback)
        };
        Self {
            ast,
            commits: ast.commits.clone(),
            git: &config.git_graph,
            theme,
            measure,
            dir: ast.direction,
            branch_pos: HashMap::new(),
            commit_pos: HashMap::new(),
            lanes: Vec::new(),
            max_pos: 0.0,
            use_redux_geometry: redux,
            use_color_theme: color_theme,
            is_dark: dark,
            neo_color_gen: neo,
            branch_font: Font::new("monospace", font_size),
            commit_label_font: Font::new(
                "monospace",
                number("commitLabelFontSize", COMMIT_LABEL_FONT_SIZE),
            ),
            tag_font: Font::new("monospace", number("tagLabelFontSize", TAG_LABEL_FONT_SIZE)),
            title_font: Font::new("monospace", TITLE_FONT_SIZE),
            branch_items: Vec::new(),
            arrow_items: Vec::new(),
            bullet_items: Vec::new(),
            label_items: Vec::new(),
        }
    }

    fn theme_string(&self, key: &str) -> Option<&str> {
        self.theme.get(key).and_then(Value::as_str)
    }

    fn color(&self, key: &str) -> Option<Color> {
        self.theme_string(key).map(Color::new)
    }

    /// `git${index % 8}`, the branch palette `normalTheme` writes.
    fn git_color(&self, index: usize) -> Option<Color> {
        self.color(&format!("git{}", index % THEME_COLOR_LIMIT))
    }

    fn stroke_width(&self) -> f64 {
        self.theme_string("strokeWidth")
            .and_then(parse_font_size)
            .unwrap_or(1.0)
    }

    fn vertical(&self) -> bool {
        matches!(self.dir, Direction::Tb | Direction::Bt)
    }

    /// The text box `getBBox()` would report for `text` in `font`: the widest row and the ink
    /// height of the first row plus the advance of every row after it.
    fn label_size(&self, text: &str, font: &Font) -> crate::scene::Size {
        let options = TextOptions {
            width: f64::INFINITY,
            markdown: false,
            min_width: 0.0,
        };
        shape_text(text, &options, font, self.measure)
            .map(|block| block.size)
            .unwrap_or_default()
    }

    /// A text run with upstream's non-HTML measurement.
    fn text_run(&self, text: &str, position: Point, font: &Font, anchor: TextAnchor) -> TextRun {
        let options = TextOptions {
            width: f64::INFINITY,
            markdown: false,
            min_width: 0.0,
        };
        let block = shape_text(text, &options, font, self.measure).unwrap_or_default();
        let mut run = TextRun::new(position, font.clone(), block);
        run.anchor = anchor;
        run
    }

    /// `drawText`: split on `\n`, a literal `\n`, and `<br>`, then trim every row.
    fn draw_text_rows(text: &str) -> Vec<String> {
        // `txt.split(/\\n|\n|<br\s*\/?>/gi)` with every row trimmed.
        let chars: Vec<char> = text.chars().collect();
        let mut rows: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut index = 0;
        while index < chars.len() {
            if chars[index] == '\\' && chars.get(index + 1) == Some(&'n') {
                rows.push(std::mem::take(&mut current));
                index += 2;
                continue;
            }
            if chars[index] == '\n' {
                rows.push(std::mem::take(&mut current));
                index += 1;
                continue;
            }
            if chars[index] == '<' {
                let rest: String = chars[index..].iter().take(6).collect();
                let lower = rest.to_ascii_lowercase();
                let mut matched = None;
                for candidate in ["<br/>", "<br />", "<br>"] {
                    if lower.starts_with(candidate) {
                        matched = Some(candidate.len());
                        break;
                    }
                }
                if let Some(length) = matched {
                    rows.push(std::mem::take(&mut current));
                    index += length;
                    continue;
                }
            }
            current.push(chars[index]);
            index += 1;
        }
        rows.push(current);
        rows.into_iter().map(|row| row.trim().to_string()).collect()
    }

    fn commit(&self, id: &str) -> Option<&Commit> {
        self.commits.iter().find(|commit| commit.id == id)
    }

    /// `calcColorIndex`.
    fn calc_color_index(raw_index: usize, limit: usize, avoid_default: bool) -> usize {
        if avoid_default && raw_index > 0 {
            return ((raw_index - 1) % (limit - 1)) + 1;
        }
        raw_index % limit
    }

    fn draw(mut self) -> Scene {
        let branches: Vec<String> = self
            .ast
            .branches_as_obj_array()
            .into_iter()
            .map(str::to_string)
            .collect();

        // First `drawCommits` pass: `draw` measures each branch label, sets `branchPos`, and
        // hands the running `pos` to the next branch.
        let mut pos = 0.0;
        for (index, name) in branches.iter().enumerate() {
            let size = self.label_size(&self.branch_text(name), &self.branch_font);
            self.branch_pos
                .insert(name.clone(), BranchPosition { pos, index });
            pos +=
                50.0 + if self.git.rotate_commit_label {
                    40.0
                } else {
                    0.0
                } + if self.vertical() {
                    size.width / 2.0
                } else {
                    0.0
                };
        }

        let order = self.layout_commits();
        if self.git.show_branches {
            self.draw_branches(&branches);
        }
        self.draw_arrows();
        self.draw_commit_shapes(&order);

        let title_bounds = self.items_bounds();
        let mut scene = Scene::default();
        if !self.branch_items.is_empty() {
            scene.push(Group {
                items: std::mem::take(&mut self.branch_items),
                ..Group::default()
            });
        }
        if !self.arrow_items.is_empty() {
            let mut group = Group::new().with_class("commit-arrows");
            group.items = std::mem::take(&mut self.arrow_items);
            scene.push(group);
        }
        if !self.bullet_items.is_empty() {
            let mut group = Group::new().with_class("commit-bullets");
            group.items = std::mem::take(&mut self.bullet_items);
            scene.push(group);
        }
        if !self.label_items.is_empty() {
            let mut group = Group::new().with_class("commit-labels");
            group.items = std::mem::take(&mut self.label_items);
            scene.push(group);
        }

        let mut bounds = title_bounds;
        if !self.ast.diagram_title.is_empty() {
            let title = self.ast.diagram_title.clone();
            let x = title_bounds.x + title_bounds.width / 2.0;
            let y = -self.git.title_top_margin;
            let mut run = self.text_run(
                &title,
                Point::new(x, y),
                &self.title_font,
                TextAnchor::Middle,
            );
            run.fill = self.color("textColor");
            let run_bounds = run.bounds();
            if run_bounds.width > 0.0 && run_bounds.height > 0.0 {
                bounds = if bounds.width > 0.0 && bounds.height > 0.0 {
                    bounds.union(run_bounds)
                } else {
                    run_bounds
                };
            }
            scene.push(run);
        }

        scene.bounds = bounds;
        scene.view_box = bounds.expanded(self.git.diagram_padding);
        scene
    }

    fn branch_text(&self, name: &str) -> String {
        Self::draw_text_rows(name).join("\n")
    }

    /// The first `drawCommits` pass: assign every commit's position. Returns the commits in the
    /// order the second pass draws them, with the position each one was drawn at.
    fn layout_commits(&mut self) -> Vec<(String, CommitPositionOffset, f64)> {
        let mut sorted_keys: Vec<String> = self
            .commits
            .iter()
            .map(|commit| commit.id.clone())
            .collect();
        sorted_keys.sort_by_key(|id| self.commit(id).map(|commit| commit.seq).unwrap_or(0));
        let parallel = self.git.parallel_commits;
        if self.dir == Direction::Bt {
            if parallel {
                self.set_parallel_bt_pos(&sorted_keys);
            }
            sorted_keys.reverse();
        }

        let mut pos = if self.vertical() { DEFAULT_POS } else { 0.0 };
        let mut order = Vec::new();
        for key in sorted_keys {
            let Some(commit) = self.commit(&key) else {
                continue;
            };
            let commit = commit.clone();
            if parallel {
                pos = self.calculate_position(&commit, pos);
            }
            let commit_position = self.get_commit_position(&commit, pos, parallel);
            if self.vertical() {
                self.commit_pos.insert(
                    commit.id.clone(),
                    CommitPosition {
                        x: commit_position.x,
                        y: commit_position.pos_with_offset,
                    },
                );
            } else {
                self.commit_pos.insert(
                    commit.id.clone(),
                    CommitPosition {
                        x: commit_position.pos_with_offset,
                        y: commit_position.y,
                    },
                );
            }
            order.push((commit.id.clone(), commit_position, pos));
            pos = if self.dir == Direction::Bt && parallel {
                pos + COMMIT_STEP
            } else {
                pos + COMMIT_STEP + LAYOUT_OFFSET
            };
            if pos > self.max_pos {
                self.max_pos = pos;
            }
        }
        order
    }

    /// `setParallelBTPos`.
    fn set_parallel_bt_pos(&mut self, sorted_keys: &[String]) {
        let mut cur_pos = DEFAULT_POS;
        let mut max_position = DEFAULT_POS;
        let mut roots: Vec<Commit> = Vec::new();

        for key in sorted_keys {
            let Some(commit) = self.commit(key).cloned() else {
                continue;
            };
            if !commit.parents.is_empty() {
                cur_pos = self.calculate_commit_position(&commit);
                max_position = cur_pos.max(max_position);
            } else {
                roots.push(commit.clone());
            }
            self.set_commit_position(&commit, cur_pos);
        }

        cur_pos = max_position;
        for root in &roots {
            self.set_root_position(root, cur_pos, DEFAULT_POS);
        }

        for key in sorted_keys {
            let Some(commit) = self.commit(key).cloned() else {
                continue;
            };
            if commit.parents.is_empty() {
                continue;
            }
            let Some(closest_parent) = self.find_closest_parent_bt(&commit.parents) else {
                continue;
            };
            let Some(parent_position) = self.commit_pos.get(&closest_parent) else {
                continue;
            };
            cur_pos = parent_position.y - COMMIT_STEP;
            if cur_pos <= max_position {
                max_position = cur_pos;
            }
            let Some(branch) = self.branch_pos.get(&commit.branch) else {
                continue;
            };
            let x = branch.pos;
            let y = cur_pos - LAYOUT_OFFSET;
            self.commit_pos
                .insert(commit.id.clone(), CommitPosition { x, y });
        }
    }

    /// `findClosestParent`: the first parent closest to the reading start along the axis.
    fn find_closest_parent(&self, parents: &[String]) -> Option<String> {
        let mut closest: Option<String> = None;
        let mut target = if self.dir == Direction::Bt {
            f64::INFINITY
        } else {
            0.0
        };
        for parent in parents {
            let Some(position) = self.commit_pos.get(parent) else {
                continue;
            };
            let value = if self.vertical() {
                position.y
            } else {
                position.x
            };
            let ok = if self.dir == Direction::Bt {
                value <= target
            } else {
                value >= target
            };
            if ok {
                closest = Some(parent.clone());
                target = value;
            }
        }
        closest
    }

    /// `findClosestParentBT`.
    fn find_closest_parent_bt(&self, parents: &[String]) -> Option<String> {
        let mut closest = String::new();
        let mut max_position = f64::INFINITY;
        for parent in parents {
            let Some(position) = self.commit_pos.get(parent) else {
                continue;
            };
            if position.y <= max_position {
                closest = parent.clone();
                max_position = position.y;
            }
        }
        (!closest.is_empty()).then_some(closest)
    }

    /// `findClosestParentPos`.
    fn find_closest_parent_pos(&self, commit: &Commit) -> f64 {
        let parents: Vec<String> = commit.parents.clone();
        let closest = self
            .find_closest_parent(&parents)
            .unwrap_or_else(|| panic!("Closest parent not found for commit {}", commit.id));
        self.commit_pos
            .get(&closest)
            .unwrap_or_else(|| panic!("Closest parent position not found for commit {}", commit.id))
            .y
    }

    /// `calculateCommitPosition`.
    fn calculate_commit_position(&self, commit: &Commit) -> f64 {
        self.find_closest_parent_pos(commit) + COMMIT_STEP
    }

    /// `setCommitPosition`.
    fn set_commit_position(&mut self, commit: &Commit, cur_pos: f64) {
        let Some(branch) = self.branch_pos.get(&commit.branch) else {
            panic!("Branch not found for commit {}", commit.id);
        };
        let position = CommitPosition {
            x: branch.pos,
            y: cur_pos + LAYOUT_OFFSET,
        };
        self.commit_pos.insert(commit.id.clone(), position);
    }

    /// `setRootPosition`.
    fn set_root_position(&mut self, commit: &Commit, cur_pos: f64, default_pos: f64) {
        let Some(branch) = self.branch_pos.get(&commit.branch) else {
            panic!("Branch not found for commit {}", commit.id);
        };
        let position = CommitPosition {
            x: branch.pos,
            y: cur_pos + default_pos,
        };
        self.commit_pos.insert(commit.id.clone(), position);
    }

    /// `calculatePosition`, the `parallelCommits` branch of `drawCommits`.
    fn calculate_position(&self, commit: &Commit, _pos: f64) -> f64 {
        let default_position = CommitPosition { x: 0.0, y: 0.0 };
        if commit.parents.is_empty() {
            return match self.dir {
                Direction::Tb => DEFAULT_POS,
                Direction::Bt => {
                    self.commit_pos
                        .get(&commit.id)
                        .copied()
                        .unwrap_or(default_position)
                        .y
                        - COMMIT_STEP
                }
                Direction::Lr => 0.0,
            };
        }
        let Some(closest_parent) = self.find_closest_parent(&commit.parents) else {
            return 0.0;
        };
        let parent_position = self
            .commit_pos
            .get(&closest_parent)
            .copied()
            .unwrap_or(default_position);
        match self.dir {
            Direction::Tb => parent_position.y + COMMIT_STEP,
            Direction::Bt => {
                self.commit_pos
                    .get(&commit.id)
                    .copied()
                    .unwrap_or(default_position)
                    .y
                    - COMMIT_STEP
            }
            Direction::Lr => parent_position.x + COMMIT_STEP,
        }
    }

    /// `getCommitPosition`.
    fn get_commit_position(
        &self,
        commit: &Commit,
        pos: f64,
        is_parallel: bool,
    ) -> CommitPositionOffset {
        let pos_with_offset = if self.dir == Direction::Bt && is_parallel {
            pos
        } else {
            pos + LAYOUT_OFFSET
        };
        let branch = self
            .branch_pos
            .get(&commit.branch)
            .unwrap_or_else(|| panic!("Position were undefined for commit {}", commit.id));
        let branch_y = branch.pos;
        let x = if self.vertical() {
            branch.pos
        } else {
            pos_with_offset
        };
        let y = if self.vertical() {
            pos_with_offset
        } else {
            branch_y
                + if self.use_redux_geometry {
                    REDUX_BRANCH_LABEL_PADDING_Y / 2.0 + 1.0
                } else {
                    -2.0
                }
        };
        CommitPositionOffset {
            x,
            y,
            pos_with_offset,
        }
    }

    /// `drawBranches`.
    fn draw_branches(&mut self, branches: &[String]) {
        let theme_color_limit = self
            .theme_string("THEME_COLOR_LIMIT")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(12);
        let color_limit = if self.use_redux_geometry {
            theme_color_limit
        } else {
            THEME_COLOR_LIMIT
        };

        for (index, name) in branches.iter().enumerate() {
            let adjust = Self::calc_color_index(index, color_limit, self.use_color_theme);
            let Some(branch) = self.branch_pos.get(name) else {
                continue;
            };
            let pos = branch.pos;
            let spine_y = if self.vertical() {
                pos
            } else if self.use_redux_geometry {
                pos + REDUX_BRANCH_LABEL_PADDING_Y / 2.0 + 1.0
            } else {
                pos - 2.0
            };

            let (start, end) = match self.dir {
                Direction::Tb => (Point::new(pos, DEFAULT_POS), Point::new(pos, self.max_pos)),
                Direction::Bt => (Point::new(pos, self.max_pos), Point::new(pos, DEFAULT_POS)),
                Direction::Lr => (Point::new(0.0, spine_y), Point::new(self.max_pos, spine_y)),
            };
            self.lanes.push(spine_y);

            let text = self.branch_text(name);
            let size = self.label_size(&text, &self.branch_font);
            let mut line_style = PathStyle {
                fill: None,
                stroke: self
                    .color("commitLineColor")
                    .or_else(|| self.color("lineColor")),
                stroke_width: self.stroke_width(),
                ..PathStyle::default()
            };
            line_style.dash = if self.neo_color_gen {
                vec![4.0, 2.0]
            } else {
                vec![2.0]
            };
            let mut line = Path::new();
            line.move_to(start);
            line.line_to(end);
            self.branch_items
                .push(PathShape::new(line, line_style).into());

            let radius = if self.use_redux_geometry { 0.0 } else { 4.0 };
            let padding_x = if self.use_redux_geometry { 16.0 } else { 0.0 };
            let padding_y = if self.use_redux_geometry {
                REDUX_BRANCH_LABEL_PADDING_Y
            } else {
                0.0
            };
            let rotate_padding = if self.git.rotate_commit_label {
                30.0
            } else {
                0.0
            };

            // `bkg`'s local rect and transform.
            let mut background_x = -size.width - 4.0 - rotate_padding;
            let mut background_y = -size.height / 2.0 + 10.0;
            let mut background_transform = Transform::IDENTITY;
            // `label`'s transform.
            let mut label_transform = Transform::translate(
                -size.width - 14.0 - rotate_padding + padding_x / 2.0,
                spine_y - size.height / 2.0 - 2.0,
            );
            match self.dir {
                Direction::Tb => {
                    background_x = pos - size.width / 2.0 - 10.0;
                    background_y = 0.0;
                    label_transform = Transform::translate(pos - size.width / 2.0 - 5.0, 0.0);
                    if self.use_redux_geometry {
                        background_transform =
                            Transform::translate(-padding_x / 2.0 - 3.0, -padding_y - 10.0);
                        label_transform = Transform::translate(
                            pos - size.width / 2.0 - 5.0,
                            -padding_y * 2.0 + 7.0,
                        );
                    }
                }
                Direction::Bt => {
                    background_x = pos - size.width / 2.0 - 10.0;
                    background_y = self.max_pos;
                    label_transform =
                        Transform::translate(pos - size.width / 2.0 - 5.0, self.max_pos);
                    if self.use_redux_geometry {
                        background_transform =
                            Transform::translate(-padding_x / 2.0 - 3.0, padding_y + 10.0);
                        label_transform = Transform::translate(
                            pos - size.width / 2.0 - 5.0,
                            self.max_pos + padding_y * 2.0 + 4.0,
                        );
                    }
                }
                Direction::Lr => {
                    background_transform =
                        Transform::translate(-19.0, spine_y - 12.0 - padding_y / 2.0);
                }
            }

            let background = Rect::new(
                background_x,
                background_y,
                size.width + 18.0 + padding_x,
                size.height + 4.0 + padding_y,
            );
            let background = translate_rect(background, background_transform);
            let mut background_style = PathStyle::filled_opt(self.branch_label_background(adjust));
            background_style.stroke_width = 0.0;
            self.branch_items
                .push(RectShape::new(background, radius, background_style).into());

            let baseline =
                label_transform.apply(Point::new(0.0, LINE_DY_EM * self.branch_font.size));
            let mut run = self.text_run(&text, baseline, &self.branch_font, TextAnchor::Start);
            run.fill = self.branch_label_text(adjust);
            self.branch_items.push(run.into());
        }
    }

    /// `.labelN`'s resolved fill (the branch label's background).
    fn branch_label_background(&self, index: usize) -> Option<Color> {
        if self.neo_color_gen {
            if self
                .theme
                .get("useGradient")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return self.color("mainBkg");
            }
            return Some(Color::new("#000000"));
        }
        if index < THEME_COLOR_LIMIT {
            self.git_color(index)
        } else {
            Some(Color::new("#000000"))
        }
    }

    /// The branch label text's fill: `.labelN` comes after `.branch-labelN` in `styles.js`, so
    /// the branch colour wins; a neo theme writes no `.labelN` rule, so its text keeps
    /// `gitBranchLabelN` (`nodeBorder` for the main branch).
    fn branch_label_text(&self, index: usize) -> Option<Color> {
        if self.neo_color_gen {
            if index == 0 {
                self.color("nodeBorder")
            } else {
                self.color(&format!("gitBranchLabel{}", index % THEME_COLOR_LIMIT))
            }
        } else if index < THEME_COLOR_LIMIT {
            self.git_color(index)
        } else {
            Some(Color::new("#000000"))
        }
    }

    /// `drawArrows`.
    fn draw_arrows(&mut self) {
        let commits = self.commits.clone();
        for commit in &commits {
            for parent in &commit.parents {
                let Some(parent_commit) = self.commit(parent) else {
                    continue;
                };
                let parent_commit = parent_commit.clone();
                self.draw_arrow(&parent_commit, commit);
            }
        }
    }

    /// `shouldRerouteArrow`.
    fn should_reroute_arrow(
        &self,
        commit_a: &Commit,
        commit_b: &Commit,
        p1: CommitPosition,
        p2: CommitPosition,
    ) -> bool {
        let commit_b_is_furthest = if self.vertical() {
            p1.x < p2.x
        } else {
            p1.y < p2.y
        };
        let branch_to_get_curve = if commit_b_is_furthest {
            &commit_b.branch
        } else {
            &commit_a.branch
        };
        self.commits.iter().any(|candidate| {
            candidate.seq > commit_a.seq
                && candidate.seq < commit_b.seq
                && &candidate.branch == branch_to_get_curve
        })
    }

    /// `findLane`, which records every lane it accepts.
    fn find_lane(&mut self, y1: f64, y2: f64, depth: u32) -> f64 {
        let candidate = y1 + (y1 - y2).abs() / 2.0;
        if depth > 5 {
            return candidate;
        }
        let ok = self
            .lanes
            .iter()
            .all(|lane| (lane - candidate).abs() >= 10.0);
        if ok {
            self.lanes.push(candidate);
            return candidate;
        }
        let difference = (y1 - y2).abs();
        self.find_lane(y1, y2 - difference / 5.0, depth + 1)
    }

    /// `drawArrow`: the path and the colour `calcColorIndex` gives it.
    fn draw_arrow(&mut self, commit_a: &Commit, commit_b: &Commit) {
        let (Some(p1), Some(p2)) = (
            self.commit_pos.get(&commit_a.id).copied(),
            self.commit_pos.get(&commit_b.id).copied(),
        ) else {
            return;
        };
        let reroute = self.should_reroute_arrow(commit_a, commit_b, p1, p2);
        let mut color_index = self
            .branch_pos
            .get(&commit_b.branch)
            .map(|branch| branch.index)
            .unwrap_or(0);
        if commit_b.kind == CommitType::Merge
            && commit_b.parents.first().map(String::as_str) != Some(commit_a.id.as_str())
        {
            color_index = self
                .branch_pos
                .get(&commit_a.branch)
                .map(|branch| branch.index)
                .unwrap_or(0);
        }

        let (radius, offset) = if reroute { (10.0, 10.0) } else { (20.0, 20.0) };
        let mut path = Path::new();
        path.move_to(Point::new(p1.x, p1.y));

        if reroute {
            let line_y = if p1.y < p2.y {
                self.find_lane(p1.y, p2.y, 0)
            } else {
                self.find_lane(p2.y, p1.y, 0)
            };
            let line_x = if p1.x < p2.x {
                self.find_lane(p1.x, p2.x, 0)
            } else {
                self.find_lane(p2.x, p1.x, 0)
            };
            match self.dir {
                Direction::Tb => {
                    if p1.x < p2.x {
                        path.line_to(Point::new(line_x - radius, p1.y));
                        path.arc_to(
                            radius,
                            radius,
                            0.0,
                            false,
                            true,
                            Point::new(line_x, p1.y + offset),
                        );
                        path.line_to(Point::new(line_x, p2.y - radius));
                        path.arc_to(
                            radius,
                            radius,
                            0.0,
                            false,
                            false,
                            Point::new(line_x + offset, p2.y),
                        );
                        path.line_to(Point::new(p2.x, p2.y));
                    } else {
                        color_index = self.branch_index(commit_a);
                        path.line_to(Point::new(line_x + radius, p1.y));
                        path.arc_to(
                            radius,
                            radius,
                            0.0,
                            false,
                            false,
                            Point::new(line_x, p1.y + offset),
                        );
                        path.line_to(Point::new(line_x, p2.y - radius));
                        path.arc_to(
                            radius,
                            radius,
                            0.0,
                            false,
                            true,
                            Point::new(line_x - offset, p2.y),
                        );
                        path.line_to(Point::new(p2.x, p2.y));
                    }
                }
                Direction::Bt => {
                    if p1.x < p2.x {
                        path.line_to(Point::new(line_x - radius, p1.y));
                        path.arc_to(
                            radius,
                            radius,
                            0.0,
                            false,
                            false,
                            Point::new(line_x, p1.y - offset),
                        );
                        path.line_to(Point::new(line_x, p2.y + radius));
                        path.arc_to(
                            radius,
                            radius,
                            0.0,
                            false,
                            true,
                            Point::new(line_x + offset, p2.y),
                        );
                        path.line_to(Point::new(p2.x, p2.y));
                    } else {
                        color_index = self.branch_index(commit_a);
                        path.line_to(Point::new(line_x + radius, p1.y));
                        path.arc_to(
                            radius,
                            radius,
                            0.0,
                            false,
                            true,
                            Point::new(line_x, p1.y - offset),
                        );
                        path.line_to(Point::new(line_x, p2.y + radius));
                        path.arc_to(
                            radius,
                            radius,
                            0.0,
                            false,
                            false,
                            Point::new(line_x - offset, p2.y),
                        );
                        path.line_to(Point::new(p2.x, p2.y));
                    }
                }
                Direction::Lr => {
                    if p1.y < p2.y {
                        path.line_to(Point::new(p1.x, line_y - radius));
                        path.arc_to(
                            radius,
                            radius,
                            0.0,
                            false,
                            false,
                            Point::new(p1.x + offset, line_y),
                        );
                        path.line_to(Point::new(p2.x - radius, line_y));
                        path.arc_to(
                            radius,
                            radius,
                            0.0,
                            false,
                            true,
                            Point::new(p2.x, line_y + offset),
                        );
                        path.line_to(Point::new(p2.x, p2.y));
                    } else {
                        color_index = self.branch_index(commit_a);
                        path.line_to(Point::new(p1.x, line_y + radius));
                        path.arc_to(
                            radius,
                            radius,
                            0.0,
                            false,
                            true,
                            Point::new(p1.x + offset, line_y),
                        );
                        path.line_to(Point::new(p2.x - radius, line_y));
                        path.arc_to(
                            radius,
                            radius,
                            0.0,
                            false,
                            false,
                            Point::new(p2.x, line_y - offset),
                        );
                        path.line_to(Point::new(p2.x, p2.y));
                    }
                }
            }
        } else {
            let merge_into_other = commit_b.kind == CommitType::Merge
                && commit_b.parents.first().map(String::as_str) != Some(commit_a.id.as_str());
            match self.dir {
                Direction::Tb => {
                    if p1.x < p2.x {
                        if merge_into_other {
                            path.line_to(Point::new(p1.x, p2.y - radius));
                            path.arc_to(
                                radius,
                                radius,
                                0.0,
                                false,
                                false,
                                Point::new(p1.x + offset, p2.y),
                            );
                            path.line_to(Point::new(p2.x, p2.y));
                        } else {
                            path.line_to(Point::new(p2.x - radius, p1.y));
                            path.arc_to(
                                radius,
                                radius,
                                0.0,
                                false,
                                true,
                                Point::new(p2.x, p1.y + offset),
                            );
                            path.line_to(Point::new(p2.x, p2.y));
                        }
                    } else if p1.x > p2.x {
                        if merge_into_other {
                            path.line_to(Point::new(p1.x, p2.y - radius));
                            path.arc_to(
                                radius,
                                radius,
                                0.0,
                                false,
                                true,
                                Point::new(p1.x - offset, p2.y),
                            );
                            path.line_to(Point::new(p2.x, p2.y));
                        } else {
                            path.line_to(Point::new(p2.x + radius, p1.y));
                            path.arc_to(
                                radius,
                                radius,
                                0.0,
                                false,
                                false,
                                Point::new(p2.x, p1.y + offset),
                            );
                            path.line_to(Point::new(p2.x, p2.y));
                        }
                    } else {
                        path.line_to(Point::new(p2.x, p2.y));
                    }
                }
                Direction::Bt => {
                    if p1.x < p2.x {
                        if merge_into_other {
                            path.line_to(Point::new(p1.x, p2.y + radius));
                            path.arc_to(
                                radius,
                                radius,
                                0.0,
                                false,
                                true,
                                Point::new(p1.x + offset, p2.y),
                            );
                            path.line_to(Point::new(p2.x, p2.y));
                        } else {
                            path.line_to(Point::new(p2.x - radius, p1.y));
                            path.arc_to(
                                radius,
                                radius,
                                0.0,
                                false,
                                false,
                                Point::new(p2.x, p1.y - offset),
                            );
                            path.line_to(Point::new(p2.x, p2.y));
                        }
                    } else if p1.x > p2.x {
                        if merge_into_other {
                            path.line_to(Point::new(p1.x, p2.y + radius));
                            path.arc_to(
                                radius,
                                radius,
                                0.0,
                                false,
                                false,
                                Point::new(p1.x - offset, p2.y),
                            );
                            path.line_to(Point::new(p2.x, p2.y));
                        } else {
                            path.line_to(Point::new(p2.x + radius, p1.y));
                            path.arc_to(
                                radius,
                                radius,
                                0.0,
                                false,
                                true,
                                Point::new(p2.x, p1.y - offset),
                            );
                            path.line_to(Point::new(p2.x, p2.y));
                        }
                    } else {
                        path.line_to(Point::new(p2.x, p2.y));
                    }
                }
                Direction::Lr => {
                    if p1.y < p2.y {
                        if merge_into_other {
                            path.line_to(Point::new(p2.x - radius, p1.y));
                            path.arc_to(
                                radius,
                                radius,
                                0.0,
                                false,
                                true,
                                Point::new(p2.x, p1.y + offset),
                            );
                            path.line_to(Point::new(p2.x, p2.y));
                        } else {
                            path.line_to(Point::new(p1.x, p2.y - radius));
                            path.arc_to(
                                radius,
                                radius,
                                0.0,
                                false,
                                false,
                                Point::new(p1.x + offset, p2.y),
                            );
                            path.line_to(Point::new(p2.x, p2.y));
                        }
                    } else if p1.y > p2.y {
                        if merge_into_other {
                            path.line_to(Point::new(p2.x - radius, p1.y));
                            path.arc_to(
                                radius,
                                radius,
                                0.0,
                                false,
                                false,
                                Point::new(p2.x, p1.y - offset),
                            );
                            path.line_to(Point::new(p2.x, p2.y));
                        } else {
                            path.line_to(Point::new(p1.x, p2.y + radius));
                            path.arc_to(
                                radius,
                                radius,
                                0.0,
                                false,
                                true,
                                Point::new(p1.x + offset, p2.y),
                            );
                            path.line_to(Point::new(p2.x, p2.y));
                        }
                    } else {
                        path.line_to(Point::new(p2.x, p2.y));
                    }
                }
            }
        }

        let index = Self::calc_color_index(color_index, THEME_COLOR_LIMIT, self.use_color_theme);
        let style = PathStyle {
            fill: None,
            stroke: self.git_color(index),
            stroke_width: if self.use_redux_geometry {
                self.stroke_width()
            } else {
                8.0
            },
            line_cap: LineCap::Round,
            ..PathStyle::default()
        };
        self.arrow_items.push(PathShape::new(path, style).into());
    }

    fn branch_index(&self, commit: &Commit) -> usize {
        self.branch_pos
            .get(&commit.branch)
            .map(|branch| branch.index)
            .unwrap_or(0)
    }

    /// The second `drawCommits` pass: draw the bullets, the labels and the tags.
    fn draw_commit_shapes(&mut self, order: &[(String, CommitPositionOffset, f64)]) {
        for (id, commit_position, pos) in order {
            let Some(commit) = self.commit(id).cloned() else {
                continue;
            };
            let type_class = get_commit_class_type(&commit);
            let symbol_type = commit.custom_type.unwrap_or(commit.kind);
            let branch_index = self.branch_index(&commit);
            self.draw_commit_bullet(&commit, *commit_position, branch_index, symbol_type);
            self.draw_commit_label(&commit, *commit_position, *pos, &type_class);
            self.draw_commit_tags(&commit, *commit_position, *pos);
        }
    }

    /// `drawCommitBullet`.
    fn draw_commit_bullet(
        &mut self,
        commit: &Commit,
        position: CommitPositionOffset,
        branch_index: usize,
        symbol_type: CommitType,
    ) {
        let redux = self.use_redux_geometry;
        let color_index =
            Self::calc_color_index(branch_index, THEME_COLOR_LIMIT, self.use_color_theme);
        match symbol_type {
            CommitType::Highlight => {
                let outer = Rect::new(
                    position.x - 10.0 + if redux { 3.0 } else { 0.0 },
                    position.y - 10.0 + if redux { 3.0 } else { 0.0 },
                    if redux { 14.0 } else { 20.0 },
                    if redux { 14.0 } else { 20.0 },
                );
                let mut outer_style = PathStyle::filled_opt(
                    self.color(&format!("gitInv{}", branch_index % THEME_COLOR_LIMIT)),
                );
                outer_style.stroke = outer_style.fill.clone();
                outer_style.stroke_width = 1.0;
                self.bullet_items.push(
                    RectShape::new(outer, 0.0, outer_style)
                        .with_source(commit_source(commit))
                        .into(),
                );

                let inner = Rect::new(
                    position.x - 6.0 + if redux { 2.0 } else { 0.0 },
                    position.y - 6.0 + if redux { 2.0 } else { 0.0 },
                    if redux { 8.0 } else { 12.0 },
                    if redux { 8.0 } else { 12.0 },
                );
                let mut inner_style = PathStyle::filled_opt(self.color("primaryColor"));
                inner_style.stroke = inner_style.fill.clone();
                inner_style.stroke_width = 1.0;
                self.bullet_items.push(
                    RectShape::new(inner, 0.0, inner_style)
                        .with_source(commit_source(commit))
                        .into(),
                );
            }
            CommitType::CherryPick => {
                let mut outer_style = PathStyle {
                    fill: self.color("nodeBorder"),
                    ..PathStyle::default()
                };
                outer_style.stroke_width = 1.0;
                self.bullet_items.push(
                    EllipseShape::new(
                        Point::new(position.x, position.y),
                        if redux { 7.0 } else { 10.0 },
                        if redux { 7.0 } else { 10.0 },
                        outer_style,
                    )
                    .with_source(commit_source(commit))
                    .into(),
                );
                let dot_fill = if self.is_dark { "#000000" } else { "#fff" };
                let radius = if redux { 2.5 } else { 2.75 };
                for dot_x in [position.x - 3.0, position.x + 3.0] {
                    self.bullet_items.push(
                        EllipseShape::new(
                            Point::new(dot_x, position.y + 2.0),
                            radius,
                            radius,
                            PathStyle::filled(dot_fill),
                        )
                        .into(),
                    );
                }
                for start_x in [position.x + 3.0, position.x - 3.0] {
                    let mut cross = Path::new();
                    cross.move_to(Point::new(start_x, position.y + 1.0));
                    cross.line_to(Point::new(position.x, position.y - 5.0));
                    let mut style = PathStyle::stroked(dot_fill, 1.0);
                    style.fill = None;
                    self.bullet_items.push(PathShape::new(cross, style).into());
                }
            }
            _ => {
                let radius = if redux { 7.0 } else { 10.0 };
                let mut circle_style = PathStyle::filled_opt(self.color("nodeBorder"));
                circle_style.stroke = self.git_color(color_index);
                circle_style.stroke_width = 1.0;
                self.bullet_items.push(
                    EllipseShape::new(
                        Point::new(position.x, position.y),
                        radius,
                        radius,
                        circle_style,
                    )
                    .with_source(commit_source(commit))
                    .into(),
                );
                if symbol_type == CommitType::Merge {
                    let radius = if redux { 5.0 } else { 6.0 };
                    let mut style = PathStyle::filled_opt(self.color("primaryColor"));
                    style.stroke = style.fill.clone();
                    style.stroke_width = 1.0;
                    self.bullet_items.push(
                        EllipseShape::new(
                            Point::new(position.x, position.y),
                            radius,
                            radius,
                            style,
                        )
                        .with_source(commit_source(commit))
                        .into(),
                    );
                }
                if symbol_type == CommitType::Reverse {
                    let value = if redux { 4.0 } else { 5.0 };
                    let mut cross = Path::new();
                    cross.move_to(Point::new(position.x - value, position.y - value));
                    cross.line_to(Point::new(position.x + value, position.y + value));
                    cross.move_to(Point::new(position.x - value, position.y + value));
                    cross.line_to(Point::new(position.x + value, position.y - value));
                    let mut style = PathStyle::filled_opt(self.color("primaryColor"));
                    style.stroke = self.color("primaryColor");
                    style.stroke_width = 3.0;
                    self.bullet_items.push(PathShape::new(cross, style).into());
                }
            }
        }
    }

    /// `drawCommitLabel`.
    fn draw_commit_label(
        &mut self,
        commit: &Commit,
        position: CommitPositionOffset,
        pos: f64,
        type_class: &str,
    ) {
        let custom_merge = commit.custom_id && commit.kind == CommitType::Merge;
        if commit.kind == CommitType::CherryPick
            || !((custom_merge) || commit.kind != CommitType::Merge)
            || !self.git.show_commit_label
        {
            return;
        }
        let size = self.label_size(&commit.id, &self.commit_label_font);
        let width = size.width + 2.0 * PY;
        let height = size.height + 2.0 * PY;
        let (rect_x, rect_y, text_x, text_y) = if self.vertical() {
            (
                position.x - (size.width + 4.0 * PX + 5.0),
                position.y - 12.0,
                position.x - (size.width + 4.0 * PX),
                position.y + size.height - 12.0,
            )
        } else {
            (
                position.pos_with_offset - size.width / 2.0 - PY,
                position.y + 13.5,
                position.pos_with_offset - size.width / 2.0,
                position.y + 25.0,
            )
        };
        let mut transform = Transform::IDENTITY;

        if self.git.rotate_commit_label {
            if self.vertical() {
                transform = Transform::rotate_about(-45.0, Point::new(position.x, position.y));
            } else {
                let r_x = -7.5 - ((size.width + 10.0) / 25.0) * 9.5;
                let r_y = 10.0 + (size.width / 25.0) * 8.5;
                transform = Transform::rotate_about(-45.0, Point::new(pos, position.y))
                    .then(Transform::translate(r_x, r_y));
            }
        }

        let mut rect_style = PathStyle {
            fill: self.color("commitLabelBackground"),
            ..PathStyle::default()
        };
        rect_style.opacity = 0.5;
        let mut text = self.text_run(
            &commit.id,
            Point::new(text_x, text_y),
            &self.commit_label_font,
            TextAnchor::Start,
        );
        text.fill = self.color("commitLabelColor");
        let mut group = Group::new()
            .with_transform(transform)
            .with_class(type_class);
        group.push(RectShape::new(
            Rect::new(rect_x, rect_y, width, height),
            0.0,
            rect_style,
        ));
        group.push(text);
        self.label_items.push(group.into());
    }

    /// `drawCommitTags`.
    fn draw_commit_tags(&mut self, commit: &Commit, position: CommitPositionOffset, pos: f64) {
        if commit.tags.is_empty() {
            return;
        }
        let mut text_elements: Vec<(String, f64)> = Vec::new();
        let mut max_width: f64 = 0.0;
        let mut max_height: f64 = 0.0;
        let mut y_offset = 0.0;
        for tag in commit.tags.iter().rev() {
            let size = self.label_size(tag, &self.tag_font);
            max_width = max_width.max(size.width);
            max_height = max_height.max(size.height);
            text_elements.push((tag.clone(), y_offset));
            y_offset += 20.0;
        }

        for (tag, y_offset) in text_elements {
            let size = self.label_size(&tag, &self.tag_font);
            let h2 = max_height / 2.0;
            let ly = position.y - 19.2 - y_offset;
            let (points, hole, text_position, transform) = if self.vertical() {
                let y_origin = pos + y_offset;
                let points = vec![
                    Point::new(position.x, y_origin + 2.0),
                    Point::new(position.x, y_origin - 2.0),
                    Point::new(position.x + LAYOUT_OFFSET, y_origin - h2 - 2.0),
                    Point::new(
                        position.x + LAYOUT_OFFSET + max_width + 4.0,
                        y_origin - h2 - 2.0,
                    ),
                    Point::new(
                        position.x + LAYOUT_OFFSET + max_width + 4.0,
                        y_origin + h2 + 2.0,
                    ),
                    Point::new(position.x + LAYOUT_OFFSET, y_origin + h2 + 2.0),
                ];
                let hole = Point::new(position.x + PX / 2.0, y_origin);
                let text = Point::new(position.x + 5.0, y_origin + 3.0);
                let transform = Transform::translate(12.0, 12.0)
                    .then(Transform::rotate_about(45.0, Point::new(position.x, pos)));
                (points, hole, text, transform)
            } else {
                let points = vec![
                    Point::new(pos - max_width / 2.0 - PX / 2.0, ly + PY),
                    Point::new(pos - max_width / 2.0 - PX / 2.0, ly - PY),
                    Point::new(
                        position.pos_with_offset - max_width / 2.0 - PX,
                        ly - h2 - PY,
                    ),
                    Point::new(
                        position.pos_with_offset + max_width / 2.0 + PX,
                        ly - h2 - PY,
                    ),
                    Point::new(
                        position.pos_with_offset + max_width / 2.0 + PX,
                        ly + h2 + PY,
                    ),
                    Point::new(
                        position.pos_with_offset - max_width / 2.0 - PX,
                        ly + h2 + PY,
                    ),
                ];
                let hole = Point::new(pos - max_width / 2.0 + PX / 2.0, ly);
                let text = Point::new(
                    position.pos_with_offset - size.width / 2.0,
                    position.y - 16.0 - y_offset,
                );
                (points, hole, text, Transform::IDENTITY)
            };

            let mut polygon = Path::new();
            for (index, point) in points.iter().enumerate() {
                if index == 0 {
                    polygon.move_to(*point);
                } else {
                    polygon.line_to(*point);
                }
            }
            polygon.close();
            let polygon_style = PathStyle {
                fill: self.color("tagLabelBackground"),
                stroke: self.color("tagLabelBorder"),
                stroke_width: 1.0,
                ..PathStyle::default()
            };
            let mut polygon_group = Group::new().with_transform(transform).with_class("tag");
            polygon_group.push(PathShape::new(polygon, polygon_style));

            let mut hole_group = Group::new().with_transform(transform);
            hole_group.push(EllipseShape::new(
                hole,
                1.5,
                1.5,
                PathStyle::filled_opt(self.color("textColor")),
            ));

            let text_transform = if self.vertical() {
                Transform::translate(14.0, 14.0)
                    .then(Transform::rotate_about(45.0, Point::new(position.x, pos)))
            } else {
                Transform::IDENTITY
            };
            let mut run = self.text_run(&tag, text_position, &self.tag_font, TextAnchor::Start);
            run.fill = self.color("tagLabelColor");
            let mut text_group = Group::new().with_transform(text_transform);
            text_group.push(run);

            self.label_items.push(polygon_group.into());
            self.label_items.push(hole_group.into());
            self.label_items.push(text_group.into());
        }
    }

    /// The bounds of everything the scene has drawn so far, with the harness's measurement
    /// rule: a box counts only when both sides are positive; a text box starts at its baseline
    /// minus its ink height; an arc is sampled.
    fn items_bounds(&self) -> Rect {
        fn include(out: &mut Option<Rect>, rect: Rect) {
            if rect.width > 0.0 && rect.height > 0.0 {
                *out = Some(out.map_or(rect, |bounds| bounds.union(rect)));
            }
        }

        fn transformed(rect: Rect, transform: Transform) -> Rect {
            let corners = [
                transform.apply(Point::new(rect.x, rect.y)),
                transform.apply(Point::new(rect.right(), rect.y)),
                transform.apply(Point::new(rect.right(), rect.bottom())),
                transform.apply(Point::new(rect.x, rect.bottom())),
            ];
            let mut bounds = Rect::new(corners[0].x, corners[0].y, 0.0, 0.0);
            for corner in &corners[1..] {
                bounds = bounds.union(Rect::new(corner.x, corner.y, 0.0, 0.0));
            }
            bounds
        }

        fn walk(item: &Item, transform: Transform, out: &mut Option<Rect>) {
            match item {
                Item::Group(group) => {
                    let transform = transform.then(group.transform);
                    for child in &group.items {
                        walk(child, transform, out);
                    }
                }
                Item::Path(shape) => {
                    if let Some(bounds) = path_bounds(&shape.path) {
                        include(out, transformed(bounds, transform));
                    }
                }
                Item::Rect(shape) => include(out, transformed(shape.rect, transform)),
                Item::Ellipse(shape) => include(
                    out,
                    transformed(
                        Rect::new(
                            shape.center.x - shape.radius_x,
                            shape.center.y - shape.radius_y,
                            shape.radius_x * 2.0,
                            shape.radius_y * 2.0,
                        ),
                        transform,
                    ),
                ),
                Item::Text(run) => include(out, transformed(run.bounds(), transform)),
                Item::Image(_) => {}
            }
        }

        let mut bounds = None;
        for item in &self.branch_items {
            walk(item, Transform::IDENTITY, &mut bounds);
        }
        for item in &self.arrow_items {
            walk(item, Transform::IDENTITY, &mut bounds);
        }
        for item in &self.bullet_items {
            walk(item, Transform::IDENTITY, &mut bounds);
        }
        for item in &self.label_items {
            walk(item, Transform::IDENTITY, &mut bounds);
        }
        bounds.unwrap_or(Rect::ZERO)
    }
}

/// The class `getCommitClassType` names, which the scene keeps on the label group.
fn get_commit_class_type(commit: &Commit) -> String {
    let symbol_type = commit.custom_type.unwrap_or(commit.kind);
    match symbol_type {
        CommitType::Normal => "commit-normal",
        CommitType::Reverse => "commit-reverse",
        CommitType::Highlight => "commit-highlight",
        CommitType::Merge => "commit-merge",
        CommitType::CherryPick => "commit-cherry-pick",
    }
    .to_string()
}

fn commit_source(commit: &Commit) -> crate::scene::SourceRef {
    crate::scene::SourceRef::node(commit.id.clone())
}

/// `translate()` applied to a rectangle's corner, the shape of `bkg.attr('transform',
/// 'translate(...)')` on a rect with `x`/`y` already set.
fn translate_rect(rect: Rect, transform: Transform) -> Rect {
    let origin = transform.apply(Point::new(rect.x, rect.y));
    Rect::new(origin.x, origin.y, rect.width, rect.height)
}

/// A path's bounding box, with arcs sampled the way `path-box.mjs` (and a browser) flattens
/// them. `dagre::svg_path_bounds` approximates arcs by their end point and radii, which is far
/// too wide for gitGraph's quarter-circle arrows.
fn path_bounds(path: &Path) -> Option<Rect> {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut include = |x: f64, y: f64| {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    };
    let mut current = Point::ZERO;
    let mut start = Point::ZERO;
    for command in &path.commands {
        match *command {
            PathCommand::MoveTo { x, y } => {
                include(x, y);
                current = Point::new(x, y);
                start = current;
            }
            PathCommand::LineTo { x, y } => {
                include(x, y);
                current = Point::new(x, y);
            }
            PathCommand::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => {
                for sample in sample_cubic(
                    current,
                    Point::new(c1x, c1y),
                    Point::new(c2x, c2y),
                    Point::new(x, y),
                ) {
                    include(sample.x, sample.y);
                }
                current = Point::new(x, y);
            }
            PathCommand::QuadTo { cx, cy, x, y } => {
                let c1 = Point::new(
                    current.x + 2.0 / 3.0 * (cx - current.x),
                    current.y + 2.0 / 3.0 * (cy - current.y),
                );
                let c2 = Point::new(x + 2.0 / 3.0 * (cx - x), y + 2.0 / 3.0 * (cy - y));
                for sample in sample_cubic(current, c1, c2, Point::new(x, y)) {
                    include(sample.x, sample.y);
                }
                current = Point::new(x, y);
            }
            PathCommand::ArcTo {
                rx,
                ry,
                rotation,
                large_arc,
                sweep,
                x,
                y,
            } => {
                for sample in sample_arc(
                    current,
                    rx,
                    ry,
                    rotation,
                    large_arc,
                    sweep,
                    Point::new(x, y),
                ) {
                    include(sample.x, sample.y);
                }
                current = Point::new(x, y);
            }
            PathCommand::Close => {
                include(start.x, start.y);
                current = start;
            }
        }
    }
    (min_x <= max_x && min_y <= max_y)
        .then(|| Rect::new(min_x, min_y, max_x - min_x, max_y - min_y))
}

fn sample_cubic(p0: Point, p1: Point, p2: Point, p3: Point) -> Vec<Point> {
    let mut points = Vec::with_capacity(18);
    for step in 0..=16 {
        let t = step as f64 / 16.0;
        let mt = 1.0 - t;
        points.push(Point::new(
            mt * mt * mt * p0.x
                + 3.0 * mt * mt * t * p1.x
                + 3.0 * mt * t * t * p2.x
                + t * t * t * p3.x,
            mt * mt * mt * p0.y
                + 3.0 * mt * mt * t * p1.y
                + 3.0 * mt * t * t * p2.y
                + t * t * t * p3.y,
        ));
    }
    points
}

/// `path-box.mjs`'s `arcExtrema`: the arc's endpoints and sixteen samples along it.
fn sample_arc(
    from: Point,
    rx: f64,
    ry: f64,
    rotation: f64,
    large_arc: bool,
    sweep: bool,
    to: Point,
) -> Vec<Point> {
    if rx == 0.0 || ry == 0.0 || (from.x - to.x).abs() < EPSILON && (from.y - to.y).abs() < EPSILON
    {
        return vec![to];
    }
    let rx = rx.abs();
    let ry = ry.abs();
    let phi = rotation.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();

    let dx = (from.x - to.x) / 2.0;
    let dy = (from.y - to.y) / 2.0;
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;

    let mut rx = rx;
    let mut ry = ry;
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let scale = lambda.sqrt();
        rx *= scale;
        ry *= scale;
    }

    let sign = if large_arc == sweep { -1.0 } else { 1.0 };
    let numerator = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let denominator = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let sq = (numerator / denominator).max(0.0);
    let coefficient = sign * sq.sqrt();
    let cxp = coefficient * (rx * y1p / ry);
    let cyp = coefficient * (-ry * x1p / rx);

    let cx = cos_phi * cxp - sin_phi * cyp + (from.x + to.x) / 2.0;
    let cy = sin_phi * cxp + cos_phi * cyp + (from.y + to.y) / 2.0;

    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| -> f64 {
        let sign = if ux * vy - uy * vx < 0.0 { -1.0 } else { 1.0 };
        let dot = ux * vx + uy * vy;
        let len = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        sign * (dot / len).clamp(-1.0, 1.0).acos()
    };

    let v1x = (x1p - cxp) / rx;
    let v1y = (y1p - cyp) / ry;
    let v2x = (-x1p - cxp) / rx;
    let v2y = (-y1p - cyp) / ry;

    let theta1 = angle(1.0, 0.0, v1x, v1y);
    let mut delta = angle(v1x, v1y, v2x, v2y);
    if !sweep && delta > 0.0 {
        delta -= 2.0 * std::f64::consts::PI;
    }
    if sweep && delta < 0.0 {
        delta += 2.0 * std::f64::consts::PI;
    }

    let mut points = vec![from, to];
    let steps = 16;
    for step in 1..steps {
        let theta = theta1 + delta * step as f64 / steps as f64;
        let (sin_t, cos_t) = theta.sin_cos();
        points.push(Point::new(
            cos_phi * rx * cos_t - sin_phi * ry * sin_t + cx,
            sin_phi * rx * cos_t + cos_phi * ry * sin_t + cy,
        ));
    }
    points
}

impl PathStyle {
    /// A style with `fill` and nothing else.
    fn filled_opt(fill: Option<Color>) -> Self {
        Self {
            fill,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::measure::FallbackMeasure;
    use crate::scene::Item;
    use serde_json::Value;

    const TOLERANCE: f64 = 0.01;

    fn fixtures() -> Vec<(String, Value)> {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/diagrams/git/tests/render");
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

    fn close_value(actual: &Value, expected: &Value, what: &str, file: &str) {
        match (actual, expected) {
            (Value::Number(actual), Value::Number(expected)) => close(
                actual.as_f64().expect("number"),
                expected.as_f64().expect("number"),
                what,
                file,
            ),
            (Value::Array(actual), Value::Array(expected)) => {
                assert_eq!(actual.len(), expected.len(), "{file}: {what}: length");
                for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                    close_value(actual, expected, &format!("{what}[{index}]"), file);
                }
            }
            (Value::Object(actual), Value::Object(expected)) => {
                assert_eq!(actual.len(), expected.len(), "{file}: {what}: keys");
                for (key, expected) in expected {
                    close_value(
                        actual.get(key).unwrap_or(&Value::Null),
                        expected,
                        &format!("{what}.{key}"),
                        file,
                    );
                }
            }
            _ => assert_eq!(actual, expected, "{file}: {what}"),
        }
    }

    fn text_of(run: &TextRun) -> String {
        run.lines
            .iter()
            .map(|line| line.text())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn fill_of(style: &PathStyle) -> &str {
        style
            .fill
            .as_ref()
            .map(|color| color.0.as_str())
            .unwrap_or_default()
    }

    fn stroke_of(style: &PathStyle) -> &str {
        style
            .stroke
            .as_ref()
            .map(|color| color.0.as_str())
            .unwrap_or_default()
    }

    fn matrix_close(transform: &Transform, expected: &Value, what: &str, file: &str) {
        let values = expected.as_array().expect("matrix");
        assert_eq!(values.len(), 6, "{file}: {what}: matrix length");
        for (index, key) in ["a", "b", "c", "d", "e", "f"].iter().enumerate() {
            let actual = match *key {
                "a" => transform.a,
                "b" => transform.b,
                "c" => transform.c,
                "d" => transform.d,
                "e" => transform.e,
                _ => transform.f,
            };
            close(
                actual,
                values[index].as_f64().expect("matrix value"),
                &format!("{what}.{key}"),
                file,
            );
        }
    }

    fn group<'a>(item: &'a Item, file: &str, what: &str) -> &'a Group {
        match item {
            Item::Group(group) => group,
            _ => panic!("{file}: {what} is not a group"),
        }
    }

    fn rect<'a>(item: &'a Item, file: &str, what: &str) -> &'a RectShape {
        match item {
            Item::Rect(rect) => rect,
            _ => panic!("{file}: {what} is not a rect"),
        }
    }

    fn text<'a>(item: &'a Item, file: &str, what: &str) -> &'a TextRun {
        match item {
            Item::Text(text) => text,
            _ => panic!("{file}: {what} is not text"),
        }
    }

    fn path<'a>(item: &'a Item, file: &str, what: &str) -> &'a PathShape {
        match item {
            Item::Path(path) => path,
            _ => panic!("{file}: {what} is not a path"),
        }
    }

    fn ellipse<'a>(item: &'a Item, file: &str, what: &str) -> &'a EllipseShape {
        match item {
            Item::Ellipse(ellipse) => ellipse,
            _ => panic!("{file}: {what} is not an ellipse"),
        }
    }

    fn scene_for(source: &str, file: &str) -> Scene {
        let preprocessed = crate::preprocess::preprocess(source);
        if let Some(error) = preprocessed.error {
            panic!("{file}: preprocess: {error}");
        }
        let config = Config::from_source_config(&preprocessed.config)
            .unwrap_or_else(|error| panic!("{file}: config: {error}"));
        let ast = super::super::parse(&preprocessed.body)
            .unwrap_or_else(|error| panic!("{file}: parse: {error}"));
        draw_ast(&ast, &config, &FallbackMeasure)
    }

    #[test]
    fn renders_recorded_git_fixtures() {
        let fixtures = fixtures();
        assert!(
            fixtures.len() >= 8,
            "expected at least 8 fixtures, found {}",
            fixtures.len()
        );

        for (file, fixture) in fixtures {
            let source = fixture["source"].as_str().expect("fixture source");
            let scene = scene_for(source, &file);
            let expected = &fixture["expected"];

            let view_box = &expected["viewBox"];
            close(
                scene.view_box.x,
                view_box["x"].as_f64().expect("viewBox x"),
                "viewBox x",
                &file,
            );
            close(
                scene.view_box.y,
                view_box["y"].as_f64().expect("viewBox y"),
                "viewBox y",
                &file,
            );
            close(
                scene.view_box.width,
                view_box["width"].as_f64().expect("viewBox width"),
                "viewBox width",
                &file,
            );
            close(
                scene.view_box.height,
                view_box["height"].as_f64().expect("viewBox height"),
                "viewBox height",
                &file,
            );

            // Branches: one group with a path, a rect and a text run per branch.
            let expected_branches = expected["branches"].as_array().expect("branches");
            let branches: Vec<&Group> = scene
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Group(group) if group.class.is_none() => Some(group),
                    _ => None,
                })
                .collect();
            if expected_branches.is_empty() {
                assert!(branches.is_empty(), "{file}: unexpected branches group");
            } else {
                assert_eq!(branches.len(), 1, "{file}: branches group count");
                let children = &branches[0].items;
                assert_eq!(
                    children.len(),
                    expected_branches.len() * 3,
                    "{file}: branch item count"
                );
                for (index, branch) in expected_branches.iter().enumerate() {
                    let line = path(&children[index * 3], &file, "branch line");
                    let expected_line = &branch["line"];
                    let commands =
                        serde_json::to_value(&line.path.commands).expect("serialize commands");
                    close_value(
                        &commands,
                        &serde_json::json!([
                            {"op": "moveTo", "x": expected_line["x1"], "y": expected_line["y1"]},
                            {"op": "lineTo", "x": expected_line["x2"], "y": expected_line["y2"]},
                        ]),
                        &format!("branch {index} line"),
                        &file,
                    );
                    assert_eq!(
                        stroke_of(&line.style),
                        expected_line["stroke"].as_str().expect("line stroke"),
                        "{file}: branch {index} stroke"
                    );
                    close(
                        line.style.stroke_width,
                        expected_line["strokeWidth"]
                            .as_f64()
                            .expect("line stroke width"),
                        &format!("branch {index} stroke width"),
                        &file,
                    );
                    let dash: Vec<f64> = expected_line["dash"]
                        .as_array()
                        .expect("line dash")
                        .iter()
                        .map(|value| value.as_f64().expect("dash value"))
                        .collect();
                    assert_eq!(line.style.dash, dash, "{file}: branch {index} dash");

                    let background = rect(&children[index * 3 + 1], &file, "branch background");
                    let expected_background = &branch["background"];
                    for (actual, expected, name) in [
                        (background.rect.x, &expected_background["x"], "x"),
                        (background.rect.y, &expected_background["y"], "y"),
                        (
                            background.rect.width,
                            &expected_background["width"],
                            "width",
                        ),
                        (
                            background.rect.height,
                            &expected_background["height"],
                            "height",
                        ),
                        (background.rx, &expected_background["rx"], "rx"),
                    ] {
                        close(
                            actual,
                            expected.as_f64().expect("background value"),
                            &format!("branch {index} background {name}"),
                            &file,
                        );
                    }
                    assert_eq!(
                        fill_of(&background.style),
                        expected_background["fill"]
                            .as_str()
                            .expect("background fill"),
                        "{file}: branch {index} background fill"
                    );

                    let label = text(&children[index * 3 + 2], &file, "branch label");
                    let expected_label = &branch["label"];
                    assert_eq!(
                        text_of(label),
                        expected_label["text"].as_str().expect("label text"),
                        "{file}: branch {index} text"
                    );
                    close(
                        label.position.x,
                        expected_label["x"].as_f64().expect("label x"),
                        &format!("branch {index} label x"),
                        &file,
                    );
                    close(
                        label.position.y,
                        expected_label["y"].as_f64().expect("label y"),
                        &format!("branch {index} label y"),
                        &file,
                    );
                    close(
                        label.font.size,
                        expected_label["fontSize"]
                            .as_f64()
                            .expect("label font size"),
                        &format!("branch {index} font size"),
                        &file,
                    );
                    assert_eq!(
                        fill_of(&PathStyle {
                            fill: label.fill.clone(),
                            ..PathStyle::default()
                        }),
                        expected_label["fill"].as_str().expect("label fill"),
                        "{file}: branch {index} label fill"
                    );
                }
            }

            // Arrows.
            let expected_arrows = expected["arrows"].as_array().expect("arrows");
            let arrows_group = scene.items.iter().find_map(|item| match item {
                Item::Group(group) if group.class.as_deref() == Some("commit-arrows") => {
                    Some(group)
                }
                _ => None,
            });
            match arrows_group {
                Some(group) => {
                    assert_eq!(
                        group.items.len(),
                        expected_arrows.len(),
                        "{file}: arrow count"
                    );
                    for (index, arrow) in expected_arrows.iter().enumerate() {
                        let shape = path(&group.items[index], &file, "arrow");
                        let commands =
                            serde_json::to_value(&shape.path.commands).expect("serialize");
                        close_value(
                            &commands,
                            &arrow["commands"],
                            &format!("arrow {index}"),
                            &file,
                        );
                        assert_eq!(
                            stroke_of(&shape.style),
                            match arrow["stroke"].as_str() {
                                None | Some("none") => "",
                                Some(value) => value,
                            },
                            "{file}: arrow {index} stroke"
                        );
                        close(
                            shape.style.stroke_width,
                            arrow["strokeWidth"].as_f64().expect("arrow width"),
                            &format!("arrow {index} width"),
                            &file,
                        );
                    }
                }
                None => assert!(expected_arrows.is_empty(), "{file}: missing arrows"),
            }

            // Bullets.
            let expected_bullets = expected["bullets"].as_array().expect("bullets");
            let bullets_group = scene.items.iter().find_map(|item| match item {
                Item::Group(group) if group.class.as_deref() == Some("commit-bullets") => {
                    Some(group)
                }
                _ => None,
            });
            match bullets_group {
                Some(group) => {
                    assert_eq!(
                        group.items.len(),
                        expected_bullets.len(),
                        "{file}: bullet count"
                    );
                    for (index, bullet) in expected_bullets.iter().enumerate() {
                        let item = &group.items[index];
                        let expected_fill = match bullet["fill"].as_str() {
                            None | Some("none") => "",
                            Some(value) => value,
                        };
                        let expected_stroke = match bullet["stroke"].as_str() {
                            None | Some("none") => "",
                            Some(value) => value,
                        };
                        match bullet["shape"].as_str().expect("bullet shape") {
                            "circle" => {
                                let circle = ellipse(item, &file, "bullet");
                                close(
                                    circle.center.x,
                                    bullet["cx"].as_f64().expect("cx"),
                                    &format!("bullet {index} cx"),
                                    &file,
                                );
                                close(
                                    circle.center.y,
                                    bullet["cy"].as_f64().expect("cy"),
                                    &format!("bullet {index} cy"),
                                    &file,
                                );
                                close(
                                    circle.radius_x,
                                    bullet["r"].as_f64().expect("r"),
                                    &format!("bullet {index} r"),
                                    &file,
                                );
                                assert_eq!(
                                    fill_of(&circle.style),
                                    expected_fill,
                                    "{file}: bullet {index} fill"
                                );
                                assert_eq!(
                                    stroke_of(&circle.style),
                                    expected_stroke,
                                    "{file}: bullet {index} stroke"
                                );
                            }
                            "rect" => {
                                let shape = rect(item, &file, "bullet");
                                for (actual, expected, name) in [
                                    (shape.rect.x, &bullet["x"], "x"),
                                    (shape.rect.y, &bullet["y"], "y"),
                                    (shape.rect.width, &bullet["width"], "width"),
                                    (shape.rect.height, &bullet["height"], "height"),
                                ] {
                                    close(
                                        actual,
                                        expected.as_f64().expect("rect value"),
                                        &format!("bullet {index} {name}"),
                                        &file,
                                    );
                                }
                                assert_eq!(
                                    fill_of(&shape.style),
                                    expected_fill,
                                    "{file}: bullet {index} fill"
                                );
                                assert_eq!(
                                    stroke_of(&shape.style),
                                    expected_stroke,
                                    "{file}: bullet {index} stroke"
                                );
                            }
                            "line" => {
                                let shape = path(item, &file, "bullet");
                                let commands =
                                    serde_json::to_value(&shape.path.commands).expect("serialize");
                                close_value(
                                    &commands,
                                    &serde_json::json!([
                                        {"op": "moveTo", "x": bullet["x1"], "y": bullet["y1"]},
                                        {"op": "lineTo", "x": bullet["x2"], "y": bullet["y2"]},
                                    ]),
                                    &format!("bullet {index}"),
                                    &file,
                                );
                                assert_eq!(
                                    stroke_of(&shape.style),
                                    expected_stroke,
                                    "{file}: bullet {index} stroke"
                                );
                            }
                            "path" => {
                                let shape = path(item, &file, "bullet");
                                let commands =
                                    serde_json::to_value(&shape.path.commands).expect("serialize");
                                close_value(
                                    &commands,
                                    &bullet["commands"],
                                    &format!("bullet {index}"),
                                    &file,
                                );
                                assert_eq!(
                                    stroke_of(&shape.style),
                                    expected_stroke,
                                    "{file}: bullet {index} stroke"
                                );
                            }
                            other => panic!("{file}: unknown bullet shape {other}"),
                        }
                    }
                }
                None => assert!(expected_bullets.is_empty(), "{file}: missing bullets"),
            }

            // Commit labels and tags share the labels group, in draw order.
            let expected_labels = expected["commitLabels"].as_array().expect("commit labels");
            let expected_tags = expected["tags"].as_array().expect("tags");
            let labels_group = scene.items.iter().find_map(|item| match item {
                Item::Group(group) if group.class.as_deref() == Some("commit-labels") => {
                    Some(group)
                }
                _ => None,
            });
            let mut label_index = 0;
            let mut tag_index = 0;
            if let Some(labels) = labels_group {
                let mut index = 0;
                while index < labels.items.len() {
                    let entry = group(&labels.items[index], &file, "label group");
                    match entry.items.first() {
                        Some(Item::Rect(_)) => {
                            let expected_label = &expected_labels[label_index];
                            matrix_close(
                                &entry.transform,
                                &expected_label["rectMatrix"],
                                &format!("commit label {label_index} rect matrix"),
                                &file,
                            );
                            matrix_close(
                                &entry.transform,
                                &expected_label["textMatrix"],
                                &format!("commit label {label_index} text matrix"),
                                &file,
                            );
                            let background =
                                rect(&entry.items[0], &file, "commit label background");
                            let expected_background = &expected_label["rect"];
                            for (actual, expected, name) in [
                                (background.rect.x, &expected_background["x"], "x"),
                                (background.rect.y, &expected_background["y"], "y"),
                                (
                                    background.rect.width,
                                    &expected_background["width"],
                                    "width",
                                ),
                                (
                                    background.rect.height,
                                    &expected_background["height"],
                                    "height",
                                ),
                            ] {
                                close(
                                    actual,
                                    expected.as_f64().expect("background value"),
                                    &format!("commit label {label_index} {name}"),
                                    &file,
                                );
                            }
                            assert_eq!(
                                fill_of(&background.style),
                                expected_background["fill"]
                                    .as_str()
                                    .expect("background fill"),
                                "{file}: commit label {label_index} background fill"
                            );
                            close(
                                background.style.opacity,
                                expected_background["opacity"].as_f64().expect("opacity"),
                                &format!("commit label {label_index} opacity"),
                                &file,
                            );
                            let run = text(&entry.items[1], &file, "commit label text");
                            let expected_text = &expected_label["text"];
                            assert_eq!(
                                text_of(run),
                                expected_text["text"].as_str().expect("label text"),
                                "{file}: commit label {label_index} text"
                            );
                            close(
                                run.position.x,
                                expected_text["x"].as_f64().expect("text x"),
                                &format!("commit label {label_index} text x"),
                                &file,
                            );
                            close(
                                run.position.y,
                                expected_text["y"].as_f64().expect("text y"),
                                &format!("commit label {label_index} text y"),
                                &file,
                            );
                            close(
                                run.font.size,
                                expected_text["fontSize"].as_f64().expect("font size"),
                                &format!("commit label {label_index} font size"),
                                &file,
                            );
                            assert_eq!(
                                fill_of(&PathStyle {
                                    fill: run.fill.clone(),
                                    ..PathStyle::default()
                                }),
                                expected_text["fill"].as_str().expect("text fill"),
                                "{file}: commit label {label_index} text fill"
                            );
                            label_index += 1;
                            index += 1;
                        }
                        Some(Item::Path(_)) => {
                            // A tag: polygon, hole and text, one group each.
                            let tag = &expected_tags[tag_index];
                            matrix_close(
                                &entry.transform,
                                &tag["polygonMatrix"],
                                &format!("tag {tag_index} polygon matrix"),
                                &file,
                            );
                            let polygon = path(&entry.items[0], &file, "tag polygon");
                            let points = tag["points"].as_array().expect("tag points");
                            let mut expected_commands = Vec::new();
                            for (point_index, point) in points.chunks(2).enumerate() {
                                expected_commands.push(serde_json::json!({
                                    "op": if point_index == 0 { "moveTo" } else { "lineTo" },
                                    "x": point[0],
                                    "y": point[1],
                                }));
                            }
                            expected_commands.push(serde_json::json!({"op": "close"}));
                            let commands =
                                serde_json::to_value(&polygon.path.commands).expect("serialize");
                            close_value(
                                &commands,
                                &Value::Array(expected_commands),
                                &format!("tag {tag_index} polygon"),
                                &file,
                            );
                            assert_eq!(
                                fill_of(&polygon.style),
                                tag["fill"].as_str().expect("tag fill"),
                                "{file}: tag {tag_index} fill"
                            );
                            assert_eq!(
                                stroke_of(&polygon.style),
                                match tag["stroke"].as_str() {
                                    None | Some("none") => "",
                                    Some(value) => value,
                                },
                                "{file}: tag {tag_index} stroke"
                            );

                            let hole_group =
                                group(&labels.items[index + 1], &file, "tag hole group");
                            matrix_close(
                                &hole_group.transform,
                                &tag["holeMatrix"],
                                &format!("tag {tag_index} hole matrix"),
                                &file,
                            );
                            let hole = ellipse(&hole_group.items[0], &file, "tag hole");
                            close(
                                hole.center.x,
                                tag["hole"]["cx"].as_f64().expect("hole cx"),
                                &format!("tag {tag_index} hole cx"),
                                &file,
                            );
                            close(
                                hole.center.y,
                                tag["hole"]["cy"].as_f64().expect("hole cy"),
                                &format!("tag {tag_index} hole cy"),
                                &file,
                            );
                            close(
                                hole.radius_x,
                                tag["hole"]["r"].as_f64().expect("hole r"),
                                &format!("tag {tag_index} hole r"),
                                &file,
                            );
                            assert_eq!(
                                fill_of(&hole.style),
                                tag["hole"]["fill"].as_str().expect("hole fill"),
                                "{file}: tag {tag_index} hole fill"
                            );

                            let text_group =
                                group(&labels.items[index + 2], &file, "tag text group");
                            matrix_close(
                                &text_group.transform,
                                &tag["textMatrix"],
                                &format!("tag {tag_index} text matrix"),
                                &file,
                            );
                            let run = text(&text_group.items[0], &file, "tag text");
                            assert_eq!(
                                text_of(run),
                                tag["text"]["text"].as_str().expect("tag text"),
                                "{file}: tag {tag_index} text"
                            );
                            close(
                                run.position.x,
                                tag["text"]["x"].as_f64().expect("tag text x"),
                                &format!("tag {tag_index} text x"),
                                &file,
                            );
                            close(
                                run.position.y,
                                tag["text"]["y"].as_f64().expect("tag text y"),
                                &format!("tag {tag_index} text y"),
                                &file,
                            );
                            close(
                                run.font.size,
                                tag["text"]["fontSize"].as_f64().expect("tag font size"),
                                &format!("tag {tag_index} font size"),
                                &file,
                            );
                            assert_eq!(
                                fill_of(&PathStyle {
                                    fill: run.fill.clone(),
                                    ..PathStyle::default()
                                }),
                                tag["text"]["fill"].as_str().expect("tag text fill"),
                                "{file}: tag {tag_index} text fill"
                            );
                            tag_index += 1;
                            index += 3;
                        }
                        _ => panic!("{file}: unexpected label group"),
                    }
                }
            }
            assert_eq!(
                label_index,
                expected_labels.len(),
                "{file}: commit label count"
            );
            assert_eq!(tag_index, expected_tags.len(), "{file}: tag count");

            // Title.
            let title = scene.items.iter().find_map(|item| match item {
                Item::Text(run) => Some(run),
                _ => None,
            });
            match &expected["title"] {
                Value::Object(expected_title) => {
                    let run = title.unwrap_or_else(|| panic!("{file}: missing title"));
                    assert_eq!(
                        text_of(run),
                        expected_title["text"].as_str().expect("title text"),
                        "{file}: title text"
                    );
                    close(
                        run.position.x,
                        expected_title["x"].as_f64().expect("title x"),
                        "title x",
                        &file,
                    );
                    close(
                        run.position.y,
                        expected_title["y"].as_f64().expect("title y"),
                        "title y",
                        &file,
                    );
                    assert_eq!(run.anchor, TextAnchor::Middle, "{file}: title anchor");
                    close(
                        run.font.size,
                        expected_title["fontSize"]
                            .as_f64()
                            .expect("title font size"),
                        "title font size",
                        &file,
                    );
                    assert_eq!(
                        fill_of(&PathStyle {
                            fill: run.fill.clone(),
                            ..PathStyle::default()
                        }),
                        expected_title["fill"].as_str().expect("title fill"),
                        "{file}: title fill"
                    );
                }
                _ => assert!(title.is_none(), "{file}: unexpected title"),
            }
        }
    }

    #[test]
    fn renders_a_basic_git_graph() {
        let scene = render(
            "gitGraph\n  commit id: \"A\"\n  commit id: \"B\"\n",
            &Config::default(),
        )
        .expect("render git graph");
        assert!(!scene.items.is_empty());
        assert!(scene.view_box.width > 0.0);
        assert!(scene.view_box.height > 0.0);
    }

    #[test]
    fn styles_branch_colors_from_the_theme() {
        let config = Config::default();
        let theme = &config.theme_variables;
        let git_color = |index: usize| {
            theme[&format!("git{index}")]
                .as_str()
                .expect("git color")
                .to_string()
        };
        let scene = render(
            "gitGraph\n  commit id: \"A\"\n  branch develop\n  commit id: \"B\"\n",
            &config,
        )
        .expect("render git graph");

        let branches = group(&scene.items[0], "scene", "branches");
        let background = rect(&branches.items[1], "scene", "branch background");
        assert_eq!(fill_of(&background.style), git_color(0));
        let label = text(&branches.items[2], "scene", "branch label");
        assert_eq!(
            label.fill.as_ref().map(|color| color.0.clone()),
            Some(git_color(0))
        );

        let background = rect(&branches.items[4], "scene", "branch background");
        assert_eq!(fill_of(&background.style), git_color(1));
        let label = text(&branches.items[5], "scene", "branch label");
        assert_eq!(
            label.fill.as_ref().map(|color| color.0.clone()),
            Some(git_color(1))
        );
    }

    #[test]
    fn render_diagram_dispatches_git_graphs() {
        let scene = crate::render_diagram(
            "gitGraph\n  commit id: \"A\"\n  commit id: \"B\"\n",
            &Config::default(),
        )
        .expect("render git graph through render_diagram");
        assert!(!scene.items.is_empty());
        assert!(scene.view_box.width > 0.0);
    }

    #[test]
    fn rejects_a_body_without_a_git_graph_header() {
        let error = render("commit id: \"A\"\n", &Config::default()).unwrap_err();
        assert!(
            error.message.contains("gitGraph"),
            "unexpected message: {}",
            error.message
        );
    }
}
