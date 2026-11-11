//! The C4 database after parsing: what upstream's `c4Db.ts` getters return (`getC4Type`,
//! `getTitle`, `getC4ShapeArray`, `getC4Shape`, `getBoundaries`, `getRels`,
//! `getC4ShapeInRow`, `getC4BoundaryInRow`, `getAccDescription`).
//!
//! Upstream stores every element as an open object: a named attribute (`$key="value"`) can
//! write any key, and `UpdateElementStyle`/`UpdateRelStyle` write the style keys by name. This
//! port keeps the keys `c4Types.ts` declares and drops a key that no type names (see the
//! module docs for the other approximations).
//!
//! `wrap` is configuration (`c4Db.setWrap`), not grammar data. This crate owns no
//! configuration, so parsing a body alone leaves it `false`, which is what upstream's own
//! specs observe before `init` runs.

/// The `text` member of a `C4Text`, or the value a quirk leaves in its place.
#[derive(Clone, Debug, PartialEq)]
pub enum TextValue {
    /// `{ text: "..." }`: the shape every C4 text field normally has.
    Text(String),
    /// A named attribute that landed in a text slot. Upstream stores the `{ key: value }`
    /// object itself, so `label.text` is then an object rather than a string.
    Named { key: String, value: String },
    /// A bare string. `UpdateElementStyle` stores its positional `techn` as a string even
    /// though the field is typed as `{ text }`, so reading `.text` on it finds nothing.
    Bare(String),
}

/// A `C4Text` field.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Text {
    pub text: TextValue,
}

impl Default for TextValue {
    fn default() -> Self {
        TextValue::Text(String::new())
    }
}

impl Text {
    /// `{ text }`.
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: TextValue::Text(text.into()),
        }
    }

    /// The field `UpdateElementStyle` leaves holding a bare string.
    pub fn bare(text: impl Into<String>) -> Self {
        Self {
            text: TextValue::Bare(text.into()),
        }
    }

    /// The string the field holds when it is a normal `{ text }`.
    pub fn as_str(&self) -> Option<&str> {
        match &self.text {
            TextValue::Text(text) => Some(text),
            TextValue::Named { .. } | TextValue::Bare(_) => None,
        }
    }
}

/// A shape stored by the grammar: a person, a system, a container, or a component
/// (`C4Shape` in `c4Types.ts`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shape {
    pub alias: String,
    pub label: Text,
    pub type_c4_shape: Text,
    pub parent_boundary: String,
    pub wrap: bool,
    pub descr: Option<Text>,
    pub techn: Option<Text>,
    /// Upstream's `type`: `$type` assigned to a shape.
    pub type_name: Option<Text>,
    pub sprite: Option<String>,
    pub tags: Option<String>,
    pub link: Option<String>,
    pub bg_color: Option<String>,
    pub font_color: Option<String>,
    pub border_color: Option<String>,
    pub shadowing: Option<String>,
    pub shape: Option<String>,
    pub legend_text: Option<String>,
    pub legend_sprite: Option<String>,
}

/// A boundary or deployment node stored by the grammar (`C4Boundary` in `c4Types.ts`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Boundary {
    pub alias: String,
    pub label: Text,
    /// Upstream's `type`; `None` when a named attribute landed in the type slot and its
    /// key was another field.
    pub type_name: Option<Text>,
    pub tags: Option<String>,
    pub link: Option<String>,
    pub parent_boundary: String,
    pub wrap: bool,
    pub descr: Option<Text>,
    pub techn: Option<Text>,
    /// `node`, `nodeL`, or `nodeR` for a deployment node (`addDeploymentNode`).
    pub node_type: Option<String>,
    pub sprite: Option<String>,
    pub bg_color: Option<String>,
    pub font_color: Option<String>,
    pub border_color: Option<String>,
    pub shadowing: Option<String>,
    pub shape: Option<String>,
    pub legend_text: Option<String>,
    pub legend_sprite: Option<String>,
}

/// A relationship stored by the grammar (`C4Rel` in `c4Types.ts`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rel {
    /// The grammar's relationship type: `rel`, `birel`, `rel_u`, `rel_d`, `rel_l`, `rel_r`,
    /// or `rel_b`.
    pub type_name: String,
    pub from: String,
    pub to: String,
    pub label: Text,
    pub techn: Option<Text>,
    pub descr: Option<Text>,
    pub sprite: Option<String>,
    pub tags: Option<String>,
    pub link: Option<String>,
    pub wrap: bool,
    pub text_color: Option<String>,
    pub line_color: Option<String>,
    pub offset_x: Option<i64>,
    pub offset_y: Option<i64>,
}

/// An entry of `getC4ShapeArray()` or `getBoundaries()`, for [`Ast::c4_shape`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum C4Element<'a> {
    Shape(&'a Shape),
    Boundary(&'a Boundary),
}

impl C4Element<'_> {
    pub fn alias(&self) -> &str {
        match self {
            C4Element::Shape(shape) => &shape.alias,
            C4Element::Boundary(boundary) => &boundary.alias,
        }
    }
}

/// What upstream's C4 database holds after parsing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    /// `getC4Type()`: the header keyword (`C4Context`, `C4Container`, ...).
    pub c4_type: Option<String>,
    /// `getTitle()`; empty when the diagram has no title.
    pub title: String,
    /// The direction a lone `direction` statement set. Upstream's action calls
    /// `yy.setDirection`, which the C4 database does not define, so this branch throws
    /// there; this port keeps the value instead.
    pub direction: Option<String>,
    /// `getC4ShapeArray()`, in declaration order.
    pub shapes: Vec<Shape>,
    /// `getBoundaries()`, in declaration order. The first entry is the `global` boundary.
    pub boundaries: Vec<Boundary>,
    /// `getRels()`, in declaration order.
    pub rels: Vec<Rel>,
    /// `getC4ShapeInRow()`; 4 until `UpdateLayoutConfig` changes it.
    pub shape_in_row: u32,
    /// `getC4BoundaryInRow()`; 2 until `UpdateLayoutConfig` changes it.
    pub boundary_in_row: u32,
    /// `getAccDescription()`. C4 never sets an accessible title, so `getAccTitle()` would
    /// always answer an empty string and is not stored.
    pub acc_description: String,
}

impl Ast {
    /// The shape with this alias, or `None`.
    pub fn shape(&self, alias: &str) -> Option<&Shape> {
        self.shapes.iter().find(|shape| shape.alias == alias)
    }

    /// The boundary with this alias, or `None` (the `global` boundary included).
    pub fn boundary(&self, alias: &str) -> Option<&Boundary> {
        self.boundaries
            .iter()
            .find(|boundary| boundary.alias == alias)
    }

    /// `getC4Shape(alias)`: the shape, falling back to a boundary, because a relationship
    /// may target a boundary as well as a shape.
    pub fn c4_shape(&self, alias: &str) -> Option<C4Element<'_>> {
        self.shape(alias)
            .map(C4Element::Shape)
            .or_else(|| self.boundary(alias).map(C4Element::Boundary))
    }

    /// `getC4ShapeArray(parentBoundary)`: the shapes whose `parentBoundary` is `parent`.
    pub fn shapes_within(&self, parent_boundary: &str) -> Vec<&Shape> {
        self.shapes
            .iter()
            .filter(|shape| shape.parent_boundary == parent_boundary)
            .collect()
    }

    /// `getBoundaries(parentBoundary)`: the boundaries whose `parentBoundary` is `parent`.
    pub fn boundaries_within(&self, parent_boundary: &str) -> Vec<&Boundary> {
        self.boundaries
            .iter()
            .filter(|boundary| boundary.parent_boundary == parent_boundary)
            .collect()
    }

    /// The relationship from `from` to `to`, or `None`.
    pub fn rel(&self, from: &str, to: &str) -> Option<&Rel> {
        self.rels
            .iter()
            .find(|rel| rel.from == from && rel.to == to)
    }
}
