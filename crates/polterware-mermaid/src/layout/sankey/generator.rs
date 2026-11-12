//! Port of `d3-sankey` 0.12.3 `src/sankey.js`.
//!
//! The generator's state lives in [`Sankey`] (upstream's closure variables) and
//! one run builds a `Graph` that the `compute*` methods mutate in place, in the
//! order upstream calls them. Upstream resolves links against a `Map` from id to
//! node; here links hold node indices, resolved once in `computeNodeLinks`.
//! The pipeline itself only reads node and link indices, so the arithmetic order and
//! the sorting order are upstream's, including the columns, which `relaxLeftToRight`
//! and `relaxRightToLeft` sort in place for the remainder of the run.

use std::collections::HashMap;

use super::align;
use super::link_horizontal;

/// Which column a node is placed in, upstream's `nodeAlign`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SankeyAlignment {
    /// `sankeyLeft`: the node's depth.
    Left,
    /// `sankeyRight`: the node's height from the right edge.
    Right,
    /// `sankeyCenter`: target nodes keep their depth, source nodes sit just before
    /// their first target.
    Center,
    /// `sankeyJustify`: source nodes sit at the right edge, the rest keep their
    /// depth. Mermaid's default.
    #[default]
    Justify,
}

impl SankeyAlignment {
    /// Reads Mermaid's `sankey.nodeAlignment` name: `left`, `right`, `center`, or
    /// `justify`.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "center" => Some(Self::Center),
            "justify" => Some(Self::Justify),
            _ => None,
        }
    }
}

/// A node entering layout, upstream's node datum.
#[derive(Clone, Debug, PartialEq)]
pub struct SankeyNodeInput {
    /// The id links name this node by.
    pub id: String,
    /// Upstream's `fixedValue`: when set, `value` is not computed from the links.
    pub fixed_value: Option<f64>,
}

impl SankeyNodeInput {
    /// A node with a value computed from its links.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            fixed_value: None,
        }
    }
}

/// A link entering layout, upstream's link datum.
#[derive(Clone, Debug, PartialEq)]
pub struct SankeyLinkInput {
    /// The id of the node the link starts at.
    pub source: String,
    /// The id of the node the link ends at.
    pub target: String,
    /// The link's flow.
    pub value: f64,
}

impl SankeyLinkInput {
    /// A link from `source` to `target` carrying `value`.
    pub fn new(source: impl Into<String>, target: impl Into<String>, value: f64) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
            value,
        }
    }
}

/// A laid-out node, upstream's node datum after layout.
#[derive(Clone, Debug, PartialEq)]
pub struct SankeyNode {
    /// The input id.
    pub id: String,
    /// Position in the input node list, upstream's `node.index`.
    pub index: usize,
    /// Flow through the node, upstream's `node.value`.
    pub value: f64,
    /// Longest path from a source node.
    pub depth: usize,
    /// Longest path to a sink node.
    pub height: usize,
    /// Column the node was placed in, upstream's `node.layer`.
    pub layer: usize,
    /// Left edge of the node's rectangle.
    pub x0: f64,
    /// Right edge of the node's rectangle.
    pub x1: f64,
    /// Top edge of the node's rectangle.
    pub y0: f64,
    /// Bottom edge of the node's rectangle.
    pub y1: f64,
}

/// A laid-out link, upstream's link datum after layout.
#[derive(Clone, Debug, PartialEq)]
pub struct SankeyLink {
    /// Position in the input link list, upstream's `link.index`.
    pub index: usize,
    /// The source node's index in [`SankeyGraph::nodes`].
    pub source: usize,
    /// The target node's index in [`SankeyGraph::nodes`].
    pub target: usize,
    /// The link's flow.
    pub value: f64,
    /// Thickness of the link at both ends.
    pub width: f64,
    /// Vertical center of the link at the source.
    pub y0: f64,
    /// Vertical center of the link at the target.
    pub y1: f64,
}

/// The result of one layout, upstream's graph object.
#[derive(Clone, Debug, PartialEq)]
pub struct SankeyGraph {
    /// Nodes in input order.
    pub nodes: Vec<SankeyNode>,
    /// Links in input order.
    pub links: Vec<SankeyLink>,
}

impl SankeyGraph {
    /// The `d` attribute `sankeyLinkHorizontal` draws for a link: a horizontal
    /// cubic Bézier from the source's right edge to the target's left edge.
    pub fn link_path(&self, link: &SankeyLink) -> String {
        link_horizontal::link_path(self, link)
    }

    /// [`SankeyGraph::link_path`] for the link at `index`.
    pub fn link_path_at(&self, index: usize) -> Option<String> {
        self.links.get(index).map(|link| self.link_path(link))
    }
}

/// Why a layout could not run, an error upstream throws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SankeyError {
    /// A link names a node id no node has, upstream's `missing: <id>`.
    MissingNode(String),
    /// The links form a cycle, upstream's `circular link`.
    CircularLink,
    /// There are no nodes, where upstream builds an array of length `NaN`.
    EmptyGraph,
}

impl std::fmt::Display for SankeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingNode(id) => write!(f, "missing: {id}"),
            Self::CircularLink => f.write_str("circular link"),
            Self::EmptyGraph => f.write_str("empty graph"),
        }
    }
}

impl std::error::Error for SankeyError {}

/// The sankey generator, upstream's closure variables as fields.
#[derive(Clone, Debug, PartialEq)]
pub struct Sankey {
    /// `extent`, as `[[x0, y0], [x1, y1]]`. Defaults to `[[0.0, 0.0], [1.0, 1.0]]`.
    pub extent: [[f64; 2]; 2],
    /// `nodeWidth`, the rectangle width. Defaults to 24.
    pub node_width: f64,
    /// `nodePadding`, the gap between nodes of a column. Defaults to 8.
    pub node_padding: f64,
    /// `nodeAlign`. Defaults to [`SankeyAlignment::Justify`].
    pub alignment: SankeyAlignment,
    /// `iterations` of the relaxation loop. Defaults to 6.
    pub iterations: usize,
}

impl Default for Sankey {
    fn default() -> Self {
        Self {
            extent: [[0.0, 0.0], [1.0, 1.0]],
            node_width: 24.0,
            node_padding: 8.0,
            alignment: SankeyAlignment::Justify,
            iterations: 6,
        }
    }
}

impl Sankey {
    /// A generator with upstream's defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Lays `nodes` and `links` out inside the extent.
    pub fn layout(
        &self,
        nodes: &[SankeyNodeInput],
        links: &[SankeyLinkInput],
    ) -> Result<SankeyGraph, SankeyError> {
        let mut graph = Graph::new(self, nodes, links)?;
        graph.compute_node_links(links)?;
        graph.compute_node_values();
        graph.compute_node_depths()?;
        graph.compute_node_heights()?;
        graph.compute_node_breadths();
        graph.compute_link_breadths();
        Ok(graph.into_graph())
    }
}

/// A node during layout, upstream's mutable node object.
#[derive(Clone, Debug)]
pub(crate) struct Node {
    pub(crate) source_links: Vec<usize>,
    pub(crate) target_links: Vec<usize>,
    pub(crate) value: f64,
    pub(crate) depth: f64,
    pub(crate) height: f64,
    pub(crate) layer: f64,
    pub(crate) x0: f64,
    pub(crate) x1: f64,
    pub(crate) y0: f64,
    pub(crate) y1: f64,
    id: String,
    index: usize,
    fixed_value: Option<f64>,
}

/// A link during layout, upstream's mutable link object.
#[derive(Clone, Debug)]
pub(crate) struct Link {
    index: usize,
    pub(crate) source: usize,
    pub(crate) target: usize,
    value: f64,
    pub(crate) width: f64,
    y0: f64,
    y1: f64,
}

/// One run of the generator, upstream's `graph` object plus the generator state
/// the pipeline reads (`x0`, `y0`, `x1`, `y1`, `dx`, `dy`, `py`, `align`,
/// `iterations`).
#[derive(Debug)]
pub(crate) struct Graph {
    pub(crate) nodes: Vec<Node>,
    pub(crate) links: Vec<Link>,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    dx: f64,
    dy: f64,
    py: f64,
    alignment: SankeyAlignment,
    iterations: usize,
}

impl Graph {
    /// `sankey()`'s first line: `{nodes: nodes.apply(...), links: links.apply(...)}`.
    fn new(
        options: &Sankey,
        nodes: &[SankeyNodeInput],
        links: &[SankeyLinkInput],
    ) -> Result<Self, SankeyError> {
        if nodes.is_empty() {
            return Err(SankeyError::EmptyGraph);
        }
        let [[x0, y0], [x1, y1]] = options.extent;
        Ok(Self {
            nodes: nodes
                .iter()
                .enumerate()
                .map(|(index, node)| Node {
                    id: node.id.clone(),
                    index,
                    source_links: Vec::new(),
                    target_links: Vec::new(),
                    value: f64::NAN,
                    depth: f64::NAN,
                    height: f64::NAN,
                    layer: f64::NAN,
                    x0: f64::NAN,
                    x1: f64::NAN,
                    y0: f64::NAN,
                    y1: f64::NAN,
                    fixed_value: node.fixed_value,
                })
                .collect(),
            links: links
                .iter()
                .enumerate()
                .map(|(index, link)| Link {
                    index,
                    source: 0,
                    target: 0,
                    value: link.value,
                    width: f64::NAN,
                    y0: f64::NAN,
                    y1: f64::NAN,
                })
                .collect(),
            x0,
            y0,
            x1,
            y1,
            dx: options.node_width,
            dy: options.node_padding,
            py: options.node_padding,
            alignment: options.alignment,
            iterations: options.iterations,
        })
    }

    /// `computeNodeLinks`: assigns indices and fills `sourceLinks`/`targetLinks`.
    fn compute_node_links(&mut self, links: &[SankeyLinkInput]) -> Result<(), SankeyError> {
        let mut node_by_id: HashMap<String, usize> = HashMap::with_capacity(self.nodes.len());
        for (index, node) in self.nodes.iter_mut().enumerate() {
            node.index = index;
            node.source_links.clear();
            node.target_links.clear();
        }
        for (index, node) in self.nodes.iter().enumerate() {
            // `new Map(nodes.map(...))`: a repeated id keeps the last node.
            node_by_id.insert(node.id.clone(), index);
        }
        for (index, link) in links.iter().enumerate() {
            let source = *node_by_id
                .get(link.source.as_str())
                .ok_or_else(|| SankeyError::MissingNode(link.source.clone()))?;
            let target = *node_by_id
                .get(link.target.as_str())
                .ok_or_else(|| SankeyError::MissingNode(link.target.clone()))?;
            self.links[index].index = index;
            self.links[index].source = source;
            self.links[index].target = target;
            self.nodes[source].source_links.push(index);
            self.nodes[target].target_links.push(index);
        }
        Ok(())
    }

    /// `computeNodeValues`.
    fn compute_node_values(&mut self) {
        for index in 0..self.nodes.len() {
            let fixed_value = self.nodes[index].fixed_value;
            let value = match fixed_value {
                Some(value) => value,
                None => {
                    let source = sum_values(
                        self.nodes[index]
                            .source_links
                            .iter()
                            .map(|&link| self.links[link].value),
                    );
                    let target = sum_values(
                        self.nodes[index]
                            .target_links
                            .iter()
                            .map(|&link| self.links[link].value),
                    );
                    max2(source, target)
                }
            };
            self.nodes[index].value = value;
        }
    }

    /// `computeNodeDepths`: breadth-first layers from the sources.
    fn compute_node_depths(&mut self) -> Result<(), SankeyError> {
        self.compute_layers(false)
    }

    /// `computeNodeHeights`: breadth-first layers from the sinks.
    fn compute_node_heights(&mut self) -> Result<(), SankeyError> {
        self.compute_layers(true)
    }

    /// The shared body of `computeNodeDepths` and `computeNodeHeights`, which only
    /// walk opposite endpoints.
    fn compute_layers(&mut self, upward: bool) -> Result<(), SankeyError> {
        let n = self.nodes.len();
        let mut current: Vec<usize> = (0..n).collect();
        let mut next: Vec<usize> = Vec::new();
        let mut queued = vec![false; n];
        let mut x = 0usize;
        while !current.is_empty() {
            for &index in &current {
                let endpoints = {
                    let node = &mut self.nodes[index];
                    if upward {
                        node.height = x as f64;
                        node.target_links.clone()
                    } else {
                        node.depth = x as f64;
                        node.source_links.clone()
                    }
                };
                for link in endpoints {
                    let visited = if upward {
                        self.links[link].source
                    } else {
                        self.links[link].target
                    };
                    if !queued[visited] {
                        queued[visited] = true;
                        next.push(visited);
                    }
                }
            }
            x += 1;
            if x > n {
                return Err(SankeyError::CircularLink);
            }
            for &index in &next {
                queued[index] = false;
            }
            current = std::mem::take(&mut next);
        }
        Ok(())
    }

    /// `computeNodeLayers`: the columns, after `nodeAlign` and before relaxation.
    ///
    /// Upstream sorts a column here only when `nodeSort` is set; Mermaid never sets
    /// it, so the columns keep node order.
    fn compute_node_layers(&mut self) -> Vec<Vec<usize>> {
        let x = max_by(&self.nodes, |node| node.depth).unwrap_or(f64::NAN) + 1.0;
        let kx = (self.x1 - self.x0 - self.dx) / (x - 1.0);
        // `new Array(x)`: x is at least 1 for a non-empty node list.
        let mut columns: Vec<Vec<usize>> = vec![Vec::new(); x as usize];
        let mut layers = Vec::with_capacity(self.nodes.len());
        for index in 0..self.nodes.len() {
            let node = &self.nodes[index];
            let aligned = match self.alignment {
                SankeyAlignment::Left => align::left(node),
                SankeyAlignment::Right => align::right(node, x),
                SankeyAlignment::Center => align::center(self, node),
                SankeyAlignment::Justify => align::justify(node, x),
            };
            let clamped = max2(0.0, min2(x - 1.0, aligned.floor()));
            // Upstream would index the columns with `NaN`; a non-finite alignment
            // is unreachable for the four functions above, so the cast saturates.
            layers.push(clamped as usize);
        }
        for (index, &layer) in layers.iter().enumerate() {
            let node = &mut self.nodes[index];
            node.layer = layer as f64;
            node.x0 = self.x0 + layer as f64 * kx;
            node.x1 = node.x0 + self.dx;
            columns[layer].push(index);
        }
        columns
    }

    /// `initializeNodeBreadths`: places the columns before relaxation.
    fn initialize_node_breadths(&mut self, columns: &[Vec<usize>]) {
        let ky = min_by(columns, |column| {
            (self.y1 - self.y0 - (column.len() as f64 - 1.0) * self.py)
                / sum_values(column.iter().map(|&index| self.nodes[index].value))
        })
        .unwrap_or(f64::NAN);
        for column in columns {
            let mut y = self.y0;
            for &index in column {
                let node = &mut self.nodes[index];
                node.y0 = y;
                node.y1 = y + node.value * ky;
                y = node.y1 + self.py;
                for link in node.source_links.clone() {
                    self.links[link].width = self.links[link].value * ky;
                }
            }
            y = (self.y1 - y + self.py) / (column.len() as f64 + 1.0);
            for (i, &index) in column.iter().enumerate() {
                let node = &mut self.nodes[index];
                node.y0 += y * (i + 1) as f64;
                node.y1 += y * (i + 1) as f64;
            }
            self.reorder_links(column);
        }
    }

    /// `computeNodeBreadths`: the columns, the vertical placement, and the
    /// relaxation loop.
    fn compute_node_breadths(&mut self) {
        let mut columns = self.compute_node_layers();
        let longest = max_by(&columns, |column| column.len() as f64).unwrap_or(f64::NAN);
        self.py = min2(self.dy, (self.y1 - self.y0) / (longest - 1.0));
        self.initialize_node_breadths(&columns);
        for i in 0..self.iterations {
            let alpha = 0.99f64.powf(i as f64);
            let beta = max2(1.0 - alpha, (i + 1) as f64 / self.iterations as f64);
            self.relax_right_to_left(&mut columns, alpha, beta);
            self.relax_left_to_right(&mut columns, alpha, beta);
        }
    }

    /// `relaxLeftToRight`: repositions each node based on its incoming links.
    fn relax_left_to_right(&mut self, columns: &mut [Vec<usize>], alpha: f64, beta: f64) {
        for column in columns.iter_mut().skip(1) {
            for &target in column.iter() {
                let mut y = 0.0;
                let mut w = 0.0;
                for k in 0..self.nodes[target].target_links.len() {
                    let link = self.nodes[target].target_links[k];
                    let source = self.links[link].source;
                    let value = self.links[link].value
                        * (self.nodes[target].layer - self.nodes[source].layer);
                    y += self.target_top(source, target) * value;
                    w += value;
                }
                if w.is_nan() || w <= 0.0 {
                    continue;
                }
                let dy = (y / w - self.nodes[target].y0) * alpha;
                self.nodes[target].y0 += dy;
                self.nodes[target].y1 += dy;
                self.reorder_node_links(target);
            }
            // `sort === undefined`: upstream sorts by breadth here, in place.
            column.sort_by(|&a, &b| {
                ascending_breadth(self.nodes[a].y0, self.nodes[b].y0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            self.resolve_collisions(column, beta);
        }
    }

    /// `relaxRightToLeft`: repositions each node based on its outgoing links.
    fn relax_right_to_left(&mut self, columns: &mut [Vec<usize>], alpha: f64, beta: f64) {
        let last = columns.len().saturating_sub(1);
        for column in columns[..last].iter_mut().rev() {
            for &source in column.iter() {
                let mut y = 0.0;
                let mut w = 0.0;
                for k in 0..self.nodes[source].source_links.len() {
                    let link = self.nodes[source].source_links[k];
                    let target = self.links[link].target;
                    let value = self.links[link].value
                        * (self.nodes[target].layer - self.nodes[source].layer);
                    y += self.source_top(source, target) * value;
                    w += value;
                }
                if w.is_nan() || w <= 0.0 {
                    continue;
                }
                let dy = (y / w - self.nodes[source].y0) * alpha;
                self.nodes[source].y0 += dy;
                self.nodes[source].y1 += dy;
                self.reorder_node_links(source);
            }
            column.sort_by(|&a, &b| {
                ascending_breadth(self.nodes[a].y0, self.nodes[b].y0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            self.resolve_collisions(column, beta);
        }
    }

    /// `resolveCollisions`: pushes overlapping nodes apart around the middle one.
    fn resolve_collisions(&mut self, nodes: &[usize], alpha: f64) {
        let i = nodes.len() >> 1;
        let subject = nodes[i];
        let (y0, y1) = (self.nodes[subject].y0, self.nodes[subject].y1);
        self.resolve_collisions_bottom_to_top(nodes, y0 - self.py, i as isize - 1, alpha);
        self.resolve_collisions_top_to_bottom(nodes, y1 + self.py, i + 1, alpha);
        self.resolve_collisions_bottom_to_top(nodes, self.y1, nodes.len() as isize - 1, alpha);
        self.resolve_collisions_top_to_bottom(nodes, self.y0, 0, alpha);
    }

    /// `resolveCollisionsTopToBottom`: pushes overlapping nodes down.
    fn resolve_collisions_top_to_bottom(
        &mut self,
        nodes: &[usize],
        mut y: f64,
        start: usize,
        alpha: f64,
    ) {
        for &index in &nodes[start.min(nodes.len())..] {
            let node = &mut self.nodes[index];
            let dy = (y - node.y0) * alpha;
            if dy > 1e-6 {
                node.y0 += dy;
                node.y1 += dy;
            }
            y = node.y1 + self.py;
        }
    }

    /// `resolveCollisionsBottomToTop`: pushes overlapping nodes up.
    fn resolve_collisions_bottom_to_top(
        &mut self,
        nodes: &[usize],
        mut y: f64,
        start: isize,
        alpha: f64,
    ) {
        let mut i = start;
        while i >= 0 {
            let node = &mut self.nodes[nodes[i as usize]];
            let dy = (node.y1 - y) * alpha;
            if dy > 1e-6 {
                node.y0 -= dy;
                node.y1 -= dy;
            }
            y = node.y0 - self.py;
            i -= 1;
        }
    }

    /// `reorderNodeLinks`: sorts the far endpoint's links after a node moves.
    fn reorder_node_links(&mut self, node: usize) {
        for link in self.nodes[node].target_links.clone() {
            let source = self.links[link].source;
            self.sort_source_links(source);
        }
        for link in self.nodes[node].source_links.clone() {
            let target = self.links[link].target;
            self.sort_target_links(target);
        }
    }

    /// `reorderLinks`: sorts both endpoint lists of every node in a column.
    fn reorder_links(&mut self, nodes: &[usize]) {
        for &node in nodes {
            self.sort_source_links(node);
            self.sort_target_links(node);
        }
    }

    /// `sourceLinks.sort(ascendingTargetBreadth)`.
    fn sort_source_links(&mut self, node: usize) {
        let mut links = std::mem::take(&mut self.nodes[node].source_links);
        let all = &self.links;
        let nodes = &self.nodes;
        links.sort_by(|&a, &b| {
            ascending_breadth_then_index(nodes[all[a].target].y0, a, nodes[all[b].target].y0, b)
        });
        self.nodes[node].source_links = links;
    }

    /// `targetLinks.sort(ascendingSourceBreadth)`.
    fn sort_target_links(&mut self, node: usize) {
        let mut links = std::mem::take(&mut self.nodes[node].target_links);
        let all = &self.links;
        let nodes = &self.nodes;
        links.sort_by(|&a, &b| {
            ascending_breadth_then_index(nodes[all[a].source].y0, a, nodes[all[b].source].y0, b)
        });
        self.nodes[node].target_links = links;
    }

    /// `targetTop`: the target `y0` that would make the link ideal.
    fn target_top(&self, source: usize, target: usize) -> f64 {
        let node = &self.nodes[source];
        let mut y = node.y0 - (node.source_links.len() as f64 - 1.0) * self.py / 2.0;
        for &link in &node.source_links {
            if self.links[link].target == target {
                break;
            }
            y += self.links[link].width + self.py;
        }
        for &link in &self.nodes[target].target_links {
            if self.links[link].source == source {
                break;
            }
            y -= self.links[link].width;
        }
        y
    }

    /// `sourceTop`: the source `y0` that would make the link ideal.
    fn source_top(&self, source: usize, target: usize) -> f64 {
        let node = &self.nodes[target];
        let mut y = node.y0 - (node.target_links.len() as f64 - 1.0) * self.py / 2.0;
        for &link in &node.target_links {
            if self.links[link].source == source {
                break;
            }
            y += self.links[link].width + self.py;
        }
        for &link in &self.nodes[source].source_links {
            if self.links[link].target == target {
                break;
            }
            y -= self.links[link].width;
        }
        y
    }

    /// `computeLinkBreadths`: centers each link on its node's end.
    fn compute_link_breadths(&mut self) {
        for index in 0..self.nodes.len() {
            let mut y0 = self.nodes[index].y0;
            let mut y1 = y0;
            for link in self.nodes[index].source_links.clone() {
                self.links[link].y0 = y0 + self.links[link].width / 2.0;
                y0 += self.links[link].width;
            }
            for link in self.nodes[index].target_links.clone() {
                self.links[link].y1 = y1 + self.links[link].width / 2.0;
                y1 += self.links[link].width;
            }
        }
    }

    /// Projects the internal nodes and links into the public result.
    fn into_graph(self) -> SankeyGraph {
        SankeyGraph {
            nodes: self
                .nodes
                .into_iter()
                .map(|node| SankeyNode {
                    id: node.id,
                    index: node.index,
                    value: node.value,
                    depth: node.depth as usize,
                    height: node.height as usize,
                    layer: node.layer as usize,
                    x0: node.x0,
                    x1: node.x1,
                    y0: node.y0,
                    y1: node.y1,
                })
                .collect(),
            links: self
                .links
                .into_iter()
                .map(|link| SankeyLink {
                    index: link.index,
                    source: link.source,
                    target: link.target,
                    value: link.value,
                    width: link.width,
                    y0: link.y0,
                    y1: link.y1,
                })
                .collect(),
        }
    }
}

/// `ascendingSourceBreadth` and `ascendingTargetBreadth`: breadth, then link index.
fn ascending_breadth_then_index(
    a: f64,
    a_index: usize,
    b: f64,
    b_index: usize,
) -> std::cmp::Ordering {
    ascending_breadth(a, b).unwrap_or_else(|| a_index.cmp(&b_index))
}

/// `ascendingBreadth`, as `a.y0 - b.y0`: `None` when the difference is falsy
/// (`0`, `-0`, or `NaN`), which is when upstream falls back to the link index.
fn ascending_breadth(a: f64, b: f64) -> Option<std::cmp::Ordering> {
    let difference = a - b;
    if difference == 0.0 || difference.is_nan() {
        None
    } else if difference < 0.0 {
        Some(std::cmp::Ordering::Less)
    } else {
        Some(std::cmp::Ordering::Greater)
    }
}

/// d3-array's `sum(values, value)`: a falsy number (`0`, `-0`, `NaN`) is skipped.
fn sum_values(values: impl Iterator<Item = f64>) -> f64 {
    let mut sum = 0.0;
    for value in values {
        if value != 0.0 && !value.is_nan() {
            sum += value;
        }
    }
    sum
}

/// d3-array's `min(values, valueof)`: `null`/`NaN` values are skipped.
fn min_by<T>(values: &[T], valueof: impl Fn(&T) -> f64) -> Option<f64> {
    let mut min: Option<f64> = None;
    for value in values {
        let value = valueof(value);
        if value.is_nan() {
            continue;
        }
        if min.is_none_or(|min| min > value) {
            min = Some(value);
        }
    }
    min
}

/// d3-array's `max(values, valueof)`: `null`/`NaN` values are skipped.
fn max_by<T>(values: &[T], valueof: impl Fn(&T) -> f64) -> Option<f64> {
    let mut max: Option<f64> = None;
    for value in values {
        let value = valueof(value);
        if value.is_nan() {
            continue;
        }
        if max.is_none_or(|max| max < value) {
            max = Some(value);
        }
    }
    max
}

/// `Math.min`: a NaN operand wins.
fn min2(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b {
        a
    } else if b < a {
        b
    } else if a.is_sign_negative() {
        a
    } else {
        b
    }
}

/// `Math.max`: a NaN operand wins.
fn max2(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else if b > a {
        b
    } else if a.is_sign_positive() {
        a
    } else {
        b
    }
}

#[cfg(test)]
mod tests {
    use super::{Sankey, SankeyError, SankeyLinkInput, SankeyNodeInput};

    fn node(id: &str) -> SankeyNodeInput {
        SankeyNodeInput::new(id)
    }

    #[test]
    fn a_link_to_an_unknown_node_reports_its_id() {
        let error = Sankey::default()
            .layout(&[node("A")], &[SankeyLinkInput::new("A", "Missing", 1.0)])
            .unwrap_err();
        assert_eq!(error, SankeyError::MissingNode("Missing".to_owned()));
        assert_eq!(error.to_string(), "missing: Missing");
    }

    #[test]
    fn a_cycle_is_an_error() {
        let error = Sankey::default()
            .layout(
                &[node("A"), node("B")],
                &[
                    SankeyLinkInput::new("A", "B", 1.0),
                    SankeyLinkInput::new("B", "A", 1.0),
                ],
            )
            .unwrap_err();
        assert_eq!(error, SankeyError::CircularLink);
        assert_eq!(error.to_string(), "circular link");
    }

    #[test]
    fn an_empty_graph_is_an_error() {
        assert_eq!(
            Sankey::default().layout(&[], &[]).unwrap_err(),
            SankeyError::EmptyGraph
        );
    }

    #[test]
    fn a_fixed_value_overrides_the_link_sum() {
        let graph = Sankey::default()
            .layout(
                &[
                    SankeyNodeInput {
                        id: "A".to_owned(),
                        fixed_value: Some(40.0),
                    },
                    node("B"),
                ],
                &[SankeyLinkInput::new("A", "B", 10.0)],
            )
            .unwrap();
        assert_eq!(graph.nodes[0].value, 40.0);
        assert_eq!(graph.nodes[1].value, 10.0);
    }
}
