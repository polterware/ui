//! Port of `src/treemap/index.js` from `d3-hierarchy` 3.1.2, the layout Mermaid 12.3.0's
//! treemap renderer runs.
//!
//! Upstream's builder stores `tile`, `round`, `dx`, `dy`, and the five padding accessors on
//! a closure, and `treemap(root)` walks the hierarchy writing rectangles. Here the builder
//! is a [`Treemap`] and the walk is [`Treemap::layout`]; padding is a [`Padding`], either a
//! constant or a function of the node, and the tiling method is a [`Tile`].
//!
//! Mermaid's renderer configures, in its own words:
//!
//! ```text
//! treemap.layout = treemap()
//!     .size([width, height])
//!     .paddingTop((d) => d.children?.length ? 35 : 0)
//!     .paddingInner(10)
//!     .paddingLeft((d) => d.children?.length ? 10 : 0)
//!     .paddingRight((d) => d.children?.length ? 10 : 0)
//!     .paddingBottom((d) => d.children?.length ? 10 : 0)
//!     .round(true);
//! ```
//!
//! which is [`Treemap::new`] plus [`Treemap::size`], those [`Padding::function`]s (closing
//! over [`crate::layout::hierarchy::Node::children`]), and [`Treemap::round`]. The default
//! tiling is [`Tile::Squarify`].
//!
//! Unlike upstream's closure, [`Treemap::layout`] can be called more than once: padding
//! depth is kept per call, while each node's `_squarify` rows (upstream's state for
//! [`Tile::Resquarify`]) deliberately survive, as they do on the node object upstream.

pub mod tile;

use std::sync::Arc;

pub use tile::Tile;

use super::{Hierarchy, Node, js_round};

/// Upstream's padding accessor: a function of the node being padded.
pub type PaddingFn<T> = dyn Fn(&Node<T>) -> f64;

/// Upstream's `paddingInner`, `paddingTop`, `paddingRight`, `paddingBottom`, or
/// `paddingLeft`: a constant, or a function of the node being padded. Mermaid uses
/// functions to give sections extra padding and leaves none.
pub enum Padding<T> {
    /// Upstream's `constant(+x)`.
    Constant(f64),
    /// Upstream's accessor function, called with the node being padded.
    Function(Arc<PaddingFn<T>>),
}

impl<T> Padding<T> {
    /// A padding of `value` for every node; upstream's `paddingInner(4)`.
    pub fn constant(value: f64) -> Self {
        Self::Constant(value)
    }

    /// A padding computed per node; upstream's `paddingTop((node) => ...)`.
    pub fn function(padding: impl Fn(&Node<T>) -> f64 + 'static) -> Self {
        Self::Function(Arc::new(padding))
    }

    /// Upstream's accessor call, `padding(node)`.
    fn get(&self, node: &Node<T>) -> f64 {
        match self {
            Self::Constant(value) => *value,
            Self::Function(padding) => padding(node),
        }
    }
}

impl<T> Clone for Padding<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Constant(value) => Self::Constant(*value),
            Self::Function(padding) => Self::Function(Arc::clone(padding)),
        }
    }
}

impl<T> From<f64> for Padding<T> {
    fn from(value: f64) -> Self {
        Self::Constant(value)
    }
}

/// Upstream's `treemap()`: the builder and the layout of a [`Hierarchy`] into rectangles.
pub struct Treemap<T> {
    tile: Tile,
    round: bool,
    dx: f64,
    dy: f64,
    padding_inner: Padding<T>,
    padding_top: Padding<T>,
    padding_right: Padding<T>,
    padding_bottom: Padding<T>,
    padding_left: Padding<T>,
}

impl<T> Default for Treemap<T> {
    fn default() -> Self {
        Self {
            tile: Tile::Squarify,
            round: false,
            dx: 1.0,
            dy: 1.0,
            padding_inner: Padding::Constant(0.0),
            padding_top: Padding::Constant(0.0),
            padding_right: Padding::Constant(0.0),
            padding_bottom: Padding::Constant(0.0),
            padding_left: Padding::Constant(0.0),
        }
    }
}

impl<T> Treemap<T> {
    /// The defaults: squarify, no rounding, a unit square, and no padding.
    pub fn new() -> Self {
        Self::default()
    }

    /// Upstream's `size([dx, dy])`.
    pub fn size(mut self, dx: f64, dy: f64) -> Self {
        self.dx = dx;
        self.dy = dy;
        self
    }

    /// Upstream's `tile(tile)`.
    pub fn tile(mut self, tile: Tile) -> Self {
        self.tile = tile;
        self
    }

    /// Upstream's `round(round)`.
    pub fn round(mut self, round: bool) -> Self {
        self.round = round;
        self
    }

    /// Upstream's `padding(x)`: the same padding inside and on all four outer edges.
    pub fn padding(mut self, padding: impl Into<Padding<T>>) -> Self {
        let padding = padding.into();
        self.padding_inner = padding.clone();
        self.padding_top = padding.clone();
        self.padding_right = padding.clone();
        self.padding_bottom = padding.clone();
        self.padding_left = padding;
        self
    }

    /// Upstream's `paddingOuter(x)`: the padding on all four outer edges.
    pub fn padding_outer(mut self, padding: impl Into<Padding<T>>) -> Self {
        let padding = padding.into();
        self.padding_top = padding.clone();
        self.padding_right = padding.clone();
        self.padding_bottom = padding.clone();
        self.padding_left = padding;
        self
    }

    /// Upstream's `paddingInner(x)`, halved for each side by the walk.
    pub fn padding_inner(mut self, padding: impl Into<Padding<T>>) -> Self {
        self.padding_inner = padding.into();
        self
    }

    /// Upstream's `paddingTop(x)`.
    pub fn padding_top(mut self, padding: impl Into<Padding<T>>) -> Self {
        self.padding_top = padding.into();
        self
    }

    /// Upstream's `paddingRight(x)`.
    pub fn padding_right(mut self, padding: impl Into<Padding<T>>) -> Self {
        self.padding_right = padding.into();
        self
    }

    /// Upstream's `paddingBottom(x)`.
    pub fn padding_bottom(mut self, padding: impl Into<Padding<T>>) -> Self {
        self.padding_bottom = padding.into();
        self
    }

    /// Upstream's `paddingLeft(x)`.
    pub fn padding_left(mut self, padding: impl Into<Padding<T>>) -> Self {
        self.padding_left = padding.into();
        self
    }

    /// Upstream's `treemap(root)`: sets the root to the configured size and lays every node
    /// out into `x0`, `y0`, `x1`, `y1`, rounded when [`Treemap::round`] is set.
    pub fn layout(&self, hierarchy: &mut Hierarchy<T>) {
        let root = hierarchy.root();
        {
            let node = hierarchy.node_mut(root);
            node.x0 = 0.0;
            node.y0 = 0.0;
            node.x1 = self.dx;
            node.y1 = self.dy;
        }

        self.position_nodes(hierarchy);

        if self.round {
            for id in hierarchy.order_each_before() {
                let node = hierarchy.node_mut(id);
                node.x0 = js_round(node.x0);
                node.y0 = js_round(node.y0);
                node.x1 = js_round(node.x1);
                node.y1 = js_round(node.y1);
            }
        }
    }

    /// Upstream's `positionNode`, applied in `eachBefore` order. `paddingStack` is local to
    /// one layout, as upstream resets it at the end of `treemap(root)`.
    fn position_nodes(&self, hierarchy: &mut Hierarchy<T>) {
        let root = hierarchy.root();
        let mut padding_stack: Vec<f64> = vec![0.0];
        let mut stack = vec![root];

        while let Some(id) = stack.pop() {
            let depth = hierarchy.node(id).depth;
            let padding = padding_stack.get(depth).copied().unwrap_or(f64::NAN);

            {
                let node = hierarchy.node_mut(id);
                let mut x0 = node.x0 + padding;
                let mut y0 = node.y0 + padding;
                let mut x1 = node.x1 - padding;
                let mut y1 = node.y1 - padding;
                if x1 < x0 {
                    x0 = (x0 + x1) / 2.0;
                    x1 = x0;
                }
                if y1 < y0 {
                    y0 = (y0 + y1) / 2.0;
                    y1 = y0;
                }
                node.x0 = x0;
                node.y0 = y0;
                node.x1 = x1;
                node.y1 = y1;
            }

            let Some(children) = hierarchy.node(id).children.clone() else {
                continue;
            };

            let inner = self.padding_inner.get(hierarchy.node(id)) / 2.0;
            if padding_stack.len() <= depth + 1 {
                padding_stack.push(inner);
            } else {
                padding_stack[depth + 1] = inner;
            }

            let (left, top, right, bottom) = {
                let node = hierarchy.node(id);
                (
                    self.padding_left.get(node),
                    self.padding_top.get(node),
                    self.padding_right.get(node),
                    self.padding_bottom.get(node),
                )
            };

            let node = hierarchy.node(id);
            let mut x0 = node.x0 + left - inner;
            let mut y0 = node.y0 + top - inner;
            let mut x1 = node.x1 - right + inner;
            let mut y1 = node.y1 - bottom + inner;
            if x1 < x0 {
                x0 = (x0 + x1) / 2.0;
                x1 = x0;
            }
            if y1 < y0 {
                y0 = (y0 + y1) / 2.0;
                y1 = y0;
            }

            self.tile.apply(hierarchy, id, x0, y0, x1, y1);

            for &child in children.iter().rev() {
                stack.push(child);
            }
        }
    }
}
