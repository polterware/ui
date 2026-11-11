//! The block database after parsing: what upstream's `BlockDB` getters answer
//! (`getBlocks`, `getBlocksFlat`, `getEdges`, `getClasses`, `getColumns`, `getBlock`).
//!
//! Upstream stores JavaScript objects with optional properties, and a few of those
//! properties can be `undefined` after a parse:
//!
//! * the id, label, and type of the block a *chained* statement leaves behind
//!   (`A --> B --> C` spreads the array of the first statement into an object, so its
//!   `id`, `label`, and `type` are all `undefined`), and
//! * the label of `root` and of the placeholder blocks a `class` statement creates for
//!   an id it has not seen (those objects are never given one).
//!
//! This port stores an absent id or label as the empty string and an absent type as
//! [`BlockType::Na`]. No block id can be empty (`NODE_ID` needs one character), so the
//! empty id cannot collide with a real one. The edge ids keep the text upstream
//! concatenates, `undefined`, because that is part of the id itself.

/// Upstream's `BlockType` (`blockTypes.ts`). The parser assigns every variant except
/// `Odd`, `OddRight`, `Ellipse`, and `Group`, which the type union keeps for other
/// diagrams and shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlockType {
    #[default]
    Na,
    ColumnSetting,
    Edge,
    Round,
    BlockArrow,
    Space,
    Square,
    Diamond,
    Hexagon,
    Odd,
    LeanRight,
    LeanLeft,
    Trapezoid,
    InvTrapezoid,
    RectLeftInvArrow,
    OddRight,
    Circle,
    Ellipse,
    Stadium,
    Subroutine,
    Cylinder,
    Group,
    DoubleCircle,
    ClassDef,
    ApplyClass,
    ApplyStyles,
    Composite,
}

impl BlockType {
    /// The string upstream stores in `block.type`.
    pub fn as_str(self) -> &'static str {
        match self {
            BlockType::Na => "na",
            BlockType::ColumnSetting => "column-setting",
            BlockType::Edge => "edge",
            BlockType::Round => "round",
            BlockType::BlockArrow => "block_arrow",
            BlockType::Space => "space",
            BlockType::Square => "square",
            BlockType::Diamond => "diamond",
            BlockType::Hexagon => "hexagon",
            BlockType::Odd => "odd",
            BlockType::LeanRight => "lean_right",
            BlockType::LeanLeft => "lean_left",
            BlockType::Trapezoid => "trapezoid",
            BlockType::InvTrapezoid => "inv_trapezoid",
            BlockType::RectLeftInvArrow => "rect_left_inv_arrow",
            BlockType::OddRight => "odd_right",
            BlockType::Circle => "circle",
            BlockType::Ellipse => "ellipse",
            BlockType::Stadium => "stadium",
            BlockType::Subroutine => "subroutine",
            BlockType::Cylinder => "cylinder",
            BlockType::Group => "group",
            BlockType::DoubleCircle => "doublecircle",
            BlockType::ClassDef => "classDef",
            BlockType::ApplyClass => "applyClass",
            BlockType::ApplyStyles => "applyStyles",
            BlockType::Composite => "composite",
        }
    }
}

/// One `ClassDef` (`getClasses()`'s value): the id and the style attributes of a
/// `classDef` statement. A later `classDef` with the same id appends to both lists.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClassDef {
    pub id: String,
    /// Every attribute, with a trailing `;` removed and whitespace trimmed
    /// (`addStyleClass`).
    pub styles: Vec<String>,
    /// The attributes that mention `color`, with `fill` renamed to `bgFill` and `color`
    /// to `fill`: the ones applied to text rather than to the shape.
    pub text_styles: Vec<String>,
}

/// One block of the database: an entry of `getBlocks()` or `getBlocksFlat()`.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    /// The id `getBlock(id)` looks the block up by. Empty where upstream's id is
    /// `undefined` (see the module docs).
    pub id: String,
    /// `block.label`. Upstream sanitizes it through `common.sanitizeText`; empty where the
    /// label is `undefined` (see the module docs).
    pub label: String,
    /// `block.type`.
    pub type_name: BlockType,
    /// `block.columns`, which only a `columns` statement inside the block sets.
    pub columns: Option<i64>,
    /// `block.widthInColumns`: the width a `:N` suffix gave the statement.
    pub width_in_columns: Option<i64>,
    /// `block.width`: the width of a `space` block.
    pub width: Option<i64>,
    /// `block.classes`, in the order the `class` statements applied them.
    pub classes: Vec<String>,
    /// `block.directions` of a block arrow, as the lexer read them (upstream does not
    /// trim them, so `( right)` keeps its leading space).
    pub directions: Vec<String>,
    /// `block.styles`, set by a `style` statement.
    pub styles: Vec<String>,
    /// `block.colorIndex`: the palette slot of a composite, in declaration order.
    pub color_index: Option<usize>,
    /// `block.children`. `None` where upstream's object has no `children` property at
    /// all (a plain block, an edge, or a placeholder); `Some` with no entries for a
    /// `space` and for the placeholders `class` creates.
    pub children: Option<Vec<Block>>,
}

/// One entry of `getEdges()`: the edge objects `populateBlockDatabase` moves out of the
/// statement list into `edgeList`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edge {
    /// The id `populateBlockDatabase` numbers: `<n>-<start>-<end>`, where `n` counts the
    /// edges between the same two ends from 1 in source order.
    pub id: String,
    /// `edge.start`. Empty where upstream's is `undefined` (see the module docs).
    pub start: String,
    /// `edge.end`. Empty where upstream's is `undefined`.
    pub end: String,
    /// `edge.label`: the text between the two halves of a labelled link, or empty.
    pub label: String,
    /// `edge.thickness`: `thick` for `==` links, `normal` otherwise.
    pub thickness: String,
    /// `edge.pattern`: `dotted` for links containing `.-`, `solid` otherwise.
    pub pattern: String,
    /// `edge.arrowTypeStart`: `arrow_point`, `arrow_cross`, `arrow_circle`, or
    /// `arrow_open` when the link carries no marker at its start.
    pub arrow_type_start: String,
    /// `edge.arrowTypeEnd`: the same names, or empty when the link has no arrow head.
    pub arrow_type_end: String,
    /// `edge.directions`: the target block's directions, copied by the grammar.
    pub directions: Vec<String>,
}

/// What upstream's block database holds after parsing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    /// `getBlocks()`: the root composite's children, in source order.
    pub blocks: Vec<Block>,
    /// `getBlocksFlat()`: every entry of the database map in insertion order, `root`
    /// first. It also holds the blocks a `class` statement created, and the original of
    /// a `space` block alongside the copies `getBlocks()` carries.
    pub flat: Vec<Block>,
    /// `getEdges()`.
    pub edges: Vec<Edge>,
    /// `getClasses()`, in the order the ids were first defined.
    pub classes: Vec<ClassDef>,
    /// What upstream sends to `log.warn` while populating the database: one message per
    /// block whose `widthInColumns` exceeds the `columns` of its own statement list. A
    /// warning is not a getter's answer; this crate has no logger, so it keeps the list.
    pub warnings: Vec<String>,
}

impl Ast {
    /// `getBlock(id)`: a lookup over [`Ast::flat`].
    pub fn block(&self, id: &str) -> Option<&Block> {
        self.flat.iter().find(|block| block.id == id)
    }

    /// `getClasses().get(id)`.
    pub fn class(&self, id: &str) -> Option<&ClassDef> {
        self.classes.iter().find(|class| class.id == id)
    }

    /// `getColumns(id)`: the block's `columns` when it has a non-zero one, otherwise the
    /// number of children it has a `children` property for, otherwise `-1`.
    pub fn columns(&self, id: &str) -> i64 {
        let Some(block) = self.block(id) else {
            return -1;
        };
        if let Some(columns) = block.columns.filter(|columns| *columns != 0) {
            return columns;
        }
        match &block.children {
            Some(children) => children.len() as i64,
            None => -1,
        }
    }
}
