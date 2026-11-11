//! The sankey database after parsing: what upstream's `sankeyDB.ts` getters return
//! (`getNodes`, `getLinks`, and the `getGraph` projection of both).
//!
//! Upstream keeps a `SankeyNode` per unique id (a `Map` for the lookup plus an array for the
//! insertion order) and a `SankeyLink` per record whose ends point at those node objects. Here
//! a node is its id and a link names the two ids, which is the shape `getGraph` answers. The
//! common database fields (`getDiagramTitle`, `getAccTitle`, `getAccDescription`) stay out of
//! the result: no rule of the sankey grammar writes them, so they are empty after a parse.

/// One node, as `findOrCreateNode` creates it (`SankeyNode`).
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// `SankeyNode.ID`, after `sanitizeText`.
    pub id: String,
}

/// One link, as `addLink` stores it (`SankeyLink`), with its ends named by id.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    /// `link.source.ID`.
    pub source: String,
    /// `link.target.ID`.
    pub target: String,
    /// `link.value`, `parseFloat(field.trim())`; `NaN` when the field is not a number.
    pub value: f64,
}

/// What upstream's sankey database holds after parsing (`getGraph()`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    /// `getNodes()`, in the order `findOrCreateNode` first saw each id.
    pub nodes: Vec<Node>,
    /// `getLinks()`, in record order.
    pub links: Vec<Link>,
}

impl Ast {
    /// The node `nodesMap.get(id)` returns.
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }
}
