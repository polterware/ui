//! Source-to-[`Scene`] rendering for architecture diagrams: the Rust side of
//! `diagrams/architecture/architectureRenderer.ts` `draw`, with `svgDraw.ts` and
//! `architectureIcons.ts`.
//!
//! Look: `architectureStyles.ts` (`.edge`, `.arrow`, `.node-bkg`, `.node-icon-text`) and the
//! attributes `drawServices` / `drawJunctions` / `drawEdges` / `drawGroups` write. The design
//! system has no architecture control, so the look is mermaid's own; the colors are the
//! theme's `archEdgeColor`, `archEdgeArrowColor`, `archEdgeWidth`, `archGroupBorderColor`,
//! `archGroupBorderWidth`, and `textColor`, and the icon tiles keep the pack's `#087ebf` and
//! `#fff`.
//!
//! Behavior: `architectureRenderer.ts` is the only consumer of cytoscape and cytoscape-fcose
//! in mermaid and gpui-kit has no architecture renderer, so the renderer itself is the model,
//! followed call by call. The layout is the [`crate::layout::fcose`] port, run with the
//! options and constraints `layoutArchitecture` builds; `architectureRenderer.ts` drives
//! cytoscape directly instead of the unified pipeline, so this module computes the cytoscape
//! state the drawing step reads:
//!
//! * node positions: every simple node starts at (0, 0) because the initial grid layout runs
//!   on an empty graph, and each compound's position and size follow its children
//!   (`updateCompoundBounds`). A compound's layout dimension is its outer size
//!   (`width + border-width + 2 * padding`), and every node's bounding box grows by one point
//!   per side for antialiasing.
//! * labels: cytoscape's invisible labels are measured under the same monospace rule as
//!   [`crate::measure::FallbackMeasure`], because `compound-sizing-wrt-labels` is `include`: a
//!   service label widens and deepens the group around it.
//! * edges: `source-endpoint`/`target-endpoint` are fractions of the node's outer size added
//!   to its center (so `L` is the center and `R` the center plus the width), the midpoint of
//!   an XY edge comes from the segment weights `layoutstop` writes, and the drawn endpoints
//!   are shifted for a `{group}` modifier and for a junction.
//!
//! Where the port stops short of upstream:
//!
//! * A [`TextRun`] carries the label's first baseline and its measured block; the SVG
//!   `dy`/`dominant-baseline` attributes are resolved into that position, and a label is one
//!   run instead of a `<text>` with one `<tspan>` per line.
//! * `foreignObject` icon text is a centered, wrapped text run in the icon's square instead of
//!   an HTML flex box; `-webkit-line-clamp` is not applied.
//! * The diagram id prefix on element ids belongs to the page; [`SourceRef`]s carry the
//!   service, junction, group, or edge id itself.
//! * `setupGraphViewbox`'s SVG width/height (`useMaxWidth`) sizes the page element, not the
//!   drawing; the scene carries the fitted viewBox.

use std::collections::HashMap;
use std::sync::Arc;

use crate::config::Config;
use crate::error::ParseError;
use crate::layout::cose_base::{AlignmentConstraint, RelativePlacementConstraint};
use crate::layout::fcose::{
    FcoseEdge, FcoseEdgeInput, FcoseEdgeValue, FcoseNode, FcoseOptions, FcosePosition,
    FcoseQuality, run_fcose_layout,
};
use crate::layout::layout_base::LayoutError as BaseLayoutError;
use crate::measure::{Font, TextMeasure, TextOptions, plain_text_block, shape_text};
use crate::render::layout_dagre::LayoutError;
use crate::scene::{
    Color, Group, Item, Path, PathShape, PathStyle, Point, Rect, RectShape, Scene, Size, SourceRef,
    TextAnchor, TextBaseline, TextRun, Transform,
};

use super::ast::{self, Alignment, AlignmentDirection, Ast, DataStructures, Direction, Node};
use super::icons;

/// The `.node-bkg` rounded rectangle a service without an icon draws: its corner radius.
pub const NODE_BKG_CORNER: f64 = 5.0;
/// `createText`'s `<text y>` constant, `-10.1`.
const TEXT_Y: f64 = -10.1;
/// `createTspan`'s line height, `1.1em`.
const LINE_HEIGHT_EM: f64 = 1.1;
/// The extra `18` of the `{group}` vertical endpoint shift.
const GROUP_LABEL_HEIGHT: f64 = 18.0;
/// `architectureStyles.ts` gives `.node-bkg` `stroke-dasharray: 8`.
const GROUP_DASH: f64 = 8.0;
/// The antialiasing expansion of every cytoscape bounding box, `expandBoundingBox(bounds, 1)`.
const BBOX_EXPANSION: f64 = 1.0;
/// Cytoscape's default `border-width`, which `outerWidth()` adds for a compound node.
const BORDER_WIDTH: f64 = 1.0;
/// `ArchitectureDirectionArrow`'s scale: `iconSize / 6`.
const ARROW_SCALE_DIVISOR: f64 = 6.0;
/// fcose's `segment-distances` default, which an edge keeps until `layoutstop` writes one.
const SEGMENT_DISTANCE: f64 = 20.0;
/// fcose's `segment-weights` default.
const SEGMENT_WEIGHT: f64 = 0.5;

/// Renders an architecture body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(
    body: &str,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, ArchitectureRenderError> {
    let ast = super::parse(body)?;
    draw_ast(&ast, config, measure)
}

/// Renders an already parsed architecture database.
pub fn draw_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, ArchitectureRenderError> {
    Renderer::new(ast, config, measure).draw()
}

/// Renders an architecture body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, ArchitectureRenderError> {
    draw(body, config, &crate::measure::FallbackMeasure)
}

/// What can stop an architecture render.
#[derive(Clone, Debug, PartialEq)]
pub enum ArchitectureRenderError {
    Parse(ParseError),
    Layout(LayoutError),
}

impl std::fmt::Display for ArchitectureRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => error.fmt(f),
            Self::Layout(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ArchitectureRenderError {}

impl From<ParseError> for ArchitectureRenderError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl From<BaseLayoutError> for ArchitectureRenderError {
    fn from(error: BaseLayoutError) -> Self {
        Self::Layout(LayoutError::Dagre(error.0.to_string()))
    }
}

/// The node kind the database stores, `data.type` in cytoscape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NodeKind {
    Group,
    Service,
    Junction,
}

/// One axis of a spatial map: the coordinate and, per group, the node ids on that row or
/// column, in `getAlignments`' insertion order.
type AxisAlignments = Vec<(i32, Vec<(String, Vec<String>)>)>;

/// A node of the cytoscape graph, in the pool order `cy.add` gives it.
struct NodeState {
    id: String,
    kind: NodeKind,
    parent: Option<usize>,
    children: Vec<usize>,
    /// The label cytoscape measures: `getServices`' and `getGroups`' title.
    label: Option<String>,
    /// `parseInt(css('padding'))`, only for a group.
    padding: f64,
}

/// The geometry cytoscape derives for one node.
#[derive(Clone, Copy, Debug)]
struct NodeGeometry {
    position: Point,
    /// `node.width()` / `node.height()`: the simple size, or a compound's auto size.
    auto: Size,
    /// `node.outerWidth()` / `outerHeight()`: the layout dimension, with border and padding.
    outer: Size,
    /// `node.boundingBox()`: the body expanded for antialiasing, without the label.
    body: Rect,
}

impl NodeGeometry {
    fn zero() -> Self {
        Self {
            position: Point::ZERO,
            auto: Size::ZERO,
            outer: Size::ZERO,
            body: Rect::ZERO,
        }
    }
}

/// The canvas-measured cytoscape label box, with `updateBoundsFromLabel`'s 2-point margin of
/// error on every side.
#[derive(Clone, Copy, Debug)]
struct LabelBox {
    width: f64,
    height: f64,
}

/// Everything `draw` needs from the database and configuration.
struct Renderer<'a> {
    config: &'a Config,
    measure: &'a dyn TextMeasure,
    font: Font,
    nodes: Vec<NodeState>,
    index: HashMap<String, usize>,
    services: HashMap<String, ast::Service>,
    group_icons: HashMap<String, Option<String>>,
    /// `getEdges()`, in declaration order.
    edges: Vec<ast::Edge>,
    data: DataStructures,
    layout_hints: Vec<ast::LayoutHint>,
    icon_size: f64,
    padding: f64,
    /// `architecture.fontSize`, the size cytoscape labels are measured with.
    label_font_size: f64,
}

impl<'a> Renderer<'a> {
    fn new(ast: &Ast, config: &'a Config, measure: &'a dyn TextMeasure) -> Self {
        let icon_size = config.architecture.icon_size;
        let padding = config.architecture.padding;
        let label_font_size = config.architecture.font_size;
        let font = Font::new("monospace", config.font_size);
        let mut nodes = Vec::new();
        let mut index = HashMap::new();

        for group in &ast.groups {
            index.insert(group.id.clone(), nodes.len());
            nodes.push(NodeState {
                id: group.id.clone(),
                kind: NodeKind::Group,
                parent: None,
                children: Vec::new(),
                label: group.title.clone(),
                padding,
            });
        }
        for node in &ast.nodes {
            let (id, label) = match node {
                Node::Service(service) => (service.id.clone(), service.title.clone()),
                Node::Junction(junction) => (junction.id.clone(), None),
            };
            index.insert(id.clone(), nodes.len());
            nodes.push(NodeState {
                id,
                kind: match node {
                    Node::Service(_) => NodeKind::Service,
                    Node::Junction(_) => NodeKind::Junction,
                },
                parent: None,
                children: Vec::new(),
                label,
                padding: 0.0,
            });
        }
        for node in &mut nodes {
            let container = ast
                .node(&node.id)
                .and_then(|node| node.container())
                .or_else(|| {
                    ast.groups
                        .iter()
                        .find(|group| group.id == node.id)
                        .and_then(|group| group.container.as_deref())
                });
            node.parent = container.and_then(|container| index.get(container).copied());
        }
        let parents: Vec<Option<usize>> = nodes.iter().map(|node| node.parent).collect();
        for (child, parent) in parents.iter().enumerate() {
            if let Some(parent) = parent {
                nodes[*parent].children.push(child);
            }
        }

        let mut services = HashMap::new();
        let mut group_icons = HashMap::new();
        for node in &ast.nodes {
            if let Node::Service(service) = node {
                services.insert(service.id.clone(), service.clone());
            }
        }
        for group in &ast.groups {
            group_icons.insert(group.id.clone(), group.icon.clone());
        }

        Self {
            config,
            measure,
            font,
            nodes,
            index,
            services,
            group_icons,
            edges: ast.edges.clone(),
            data: ast.data_structures(),
            layout_hints: ast.layout_hints.clone(),
            icon_size,
            padding,
            label_font_size,
        }
    }

    fn draw(&self) -> Result<Scene, ArchitectureRenderError> {
        let initial = self.compound_geometry(&vec![Point::ZERO; self.nodes.len()]);
        let first = self.place(&initial, None)?;
        // `layout.one('layoutstop', ...)` writes the segment weights from the first run's
        // endpoints and runs the layout a second time from the positions that run applied.
        // The seed resets for each `run()`, so the two runs see the same random stream.
        let segments: Vec<SegmentStyle> = self
            .edges
            .iter()
            .map(|edge| self.edge_segment(edge, &first))
            .collect();
        let geometry = self.place(&first, Some(&segments))?;

        let mut scene = Scene::default();
        let mut edges = Group::new().with_class("architecture-edges");
        for (index, edge) in self.edges.iter().enumerate() {
            self.draw_edge(&mut edges, edge, &geometry, &segments[index]);
        }
        scene.push(edges);

        let mut services = Group::new().with_class("architecture-services");
        for (node, state) in self.nodes.iter().enumerate() {
            match state.kind {
                NodeKind::Service => self.draw_service(&mut services, state, &geometry[node]),
                NodeKind::Junction => self.draw_junction(&mut services, state, &geometry[node]),
                NodeKind::Group => {}
            }
        }
        scene.push(services);

        let mut groups = Group::new().with_class("architecture-groups");
        for (node, state) in self.nodes.iter().enumerate() {
            if state.kind == NodeKind::Group {
                self.draw_group(&mut groups, node, state, &geometry[node]);
            }
        }
        scene.push(groups);

        let bounds = scene_bounds(&scene.items).unwrap_or(Rect::ZERO);
        scene.bounds = bounds;
        scene.view_box = bounds.expanded(self.padding);
        Ok(scene)
    }

    // ---- cytoscape state ----

    /// `labelWidth`/`labelHeight` of a cytoscape node label: the canvas measurement under the
    /// monospace rule plus `updateBoundsFromLabel`'s margin of error.
    fn label_box(&self, label: &str) -> LabelBox {
        let font = Font::new("monospace", self.label_font_size);
        let ink = self.measure.width(label, &font).ceil();
        LabelBox {
            width: ink + 4.0,
            height: self.label_font_size + 4.0,
        }
    }

    /// The box a node's label occupies, `updateBoundsFromLabel` for `text-valign: bottom`,
    /// `text-halign: center`.
    fn label_rect(&self, label: &str, position: Point, height: f64) -> Rect {
        let measured = self.label_box(label);
        Rect::new(
            position.x - measured.width / 2.0,
            position.y + height / 2.0 - 2.0,
            measured.width,
            measured.height,
        )
    }

    /// The box a compound's `children.boundingBox({ includeLabels })` unions: each child's
    /// body already expanded one point per side, and its label box without that expansion.
    fn compound_child_bbox(
        &self,
        state: &NodeState,
        position: Point,
        auto_height: f64,
        outer: Size,
    ) -> Rect {
        let body = Rect::new(
            position.x - outer.width / 2.0,
            position.y - outer.height / 2.0,
            outer.width,
            outer.height,
        )
        .expanded(BBOX_EXPANSION);
        match &state.label {
            Some(label) if !label.is_empty() => {
                body.union(self.label_rect(label, position, auto_height))
            }
            _ => body,
        }
    }

    /// `boundingBoxImpl` for one node: the outer body, the label when the node has one, and
    /// the one-point antialiasing expansion applied after the two are united.
    fn bbox_with_label(
        &self,
        state: &NodeState,
        position: Point,
        auto_height: f64,
        outer: Size,
    ) -> Rect {
        let body = Rect::new(
            position.x - outer.width / 2.0,
            position.y - outer.height / 2.0,
            outer.width,
            outer.height,
        );
        let bounds = match &state.label {
            Some(label) if !label.is_empty() => {
                body.union(self.label_rect(label, position, auto_height))
            }
            _ => body,
        };
        bounds.expanded(BBOX_EXPANSION)
    }

    /// `updateCompoundBounds` for every group, innermost first. A compound's position is the
    /// center of its children's bounding boxes, its auto size that union's size, and its outer
    /// size adds the default border and the padding on both sides.
    fn fill_compounds(&self, mut geometry: Vec<NodeGeometry>) -> Vec<NodeGeometry> {
        for node in self.groups_innermost_first() {
            let state = &self.nodes[node];
            let mut bounds: Option<Rect> = None;
            for &child in &state.children {
                let child_state = &self.nodes[child];
                let child_box = self.compound_child_bbox(
                    child_state,
                    geometry[child].position,
                    geometry[child].auto.height,
                    geometry[child].outer,
                );
                bounds = Some(match bounds {
                    Some(bounds) => bounds.union(child_box),
                    None => child_box,
                });
            }
            let Some(bounds) = bounds else {
                geometry[node].position = Point::ZERO;
                geometry[node].auto = Size::new(1.0, 1.0);
                geometry[node].outer = Size::new(
                    1.0 + BORDER_WIDTH + 2.0 * state.padding,
                    1.0 + BORDER_WIDTH + 2.0 * state.padding,
                );
                geometry[node].body = self.bbox_with_label(
                    state,
                    geometry[node].position,
                    geometry[node].auto.height,
                    geometry[node].outer,
                );
                continue;
            };
            geometry[node].position = bounds.center();
            geometry[node].auto = Size::new(bounds.width, bounds.height);
            geometry[node].outer = Size::new(
                bounds.width + BORDER_WIDTH + 2.0 * state.padding,
                bounds.height + BORDER_WIDTH + 2.0 * state.padding,
            );
            geometry[node].body = self.bbox_with_label(
                state,
                geometry[node].position,
                geometry[node].auto.height,
                geometry[node].outer,
            );
        }
        geometry
    }

    /// The compound pass for a set of simple positions, used for the layout input.
    fn compound_geometry(&self, positions: &[Point]) -> Vec<NodeGeometry> {
        let mut geometry = vec![NodeGeometry::zero(); self.nodes.len()];
        for (node, state) in self.nodes.iter().enumerate() {
            let size = if state.kind == NodeKind::Group {
                Size::ZERO
            } else {
                Size::new(self.icon_size, self.icon_size)
            };
            geometry[node].position = positions[node];
            geometry[node].auto = size;
            geometry[node].outer = size;
            geometry[node].body = if state.kind == NodeKind::Group {
                Rect::ZERO
            } else {
                self.bbox_with_label(state, positions[node], size.height, size)
            };
        }
        self.fill_compounds(geometry)
    }

    /// Groups, deepest first; ties keep declaration order.
    fn groups_innermost_first(&self) -> Vec<usize> {
        let mut groups: Vec<usize> = (0..self.nodes.len())
            .filter(|&node| self.nodes[node].kind == NodeKind::Group)
            .collect();
        groups.sort_by_key(|&node| std::cmp::Reverse(self.depth(node)));
        groups
    }

    fn depth(&self, mut node: usize) -> usize {
        let mut depth = 0;
        while let Some(parent) = self.nodes[node].parent {
            depth += 1;
            node = parent;
        }
        depth
    }

    /// The manual endpoint of `node`: the direction's fraction of the node's outer size added
    /// to its center (`'0 50%'`, `'100% 50%'`, `'50% 0'`, `'50% 100%'`).
    fn node_endpoint(&self, node: &str, direction: Direction, geometry: &[NodeGeometry]) -> Point {
        let index = self.index[node];
        let outer = geometry[index].outer;
        let position = geometry[index].position;
        match direction {
            Direction::Left => Point::new(position.x, position.y + outer.height / 2.0),
            Direction::Right => {
                Point::new(position.x + outer.width, position.y + outer.height / 2.0)
            }
            Direction::Top => Point::new(position.x + outer.width / 2.0, position.y),
            Direction::Bottom => {
                Point::new(position.x + outer.width / 2.0, position.y + outer.height)
            }
        }
    }

    fn node_kind(&self, id: &str) -> Option<NodeKind> {
        self.index.get(id).map(|&index| self.nodes[index].kind)
    }

    // ---- layout ----

    /// Runs fcose from the node positions and sizes `geometry` holds, then recomputes the
    /// compounds from the result, which is the state cytoscape holds between two runs.
    fn place(
        &self,
        geometry: &[NodeGeometry],
        segments: Option<&[SegmentStyle]>,
    ) -> Result<Vec<NodeGeometry>, ArchitectureRenderError> {
        let positions = self.run_layout(geometry, segments)?;
        let mut placed = vec![NodeGeometry::zero(); self.nodes.len()];
        for (node, state) in self.nodes.iter().enumerate() {
            let position = positions
                .iter()
                .find(|position| position.id == state.id)
                .map(|position| Point::new(position.x, position.y))
                .unwrap_or(geometry[node].position);
            let simple = if state.kind == NodeKind::Group {
                Size::ZERO
            } else {
                Size::new(self.icon_size, self.icon_size)
            };
            placed[node].position = position;
            placed[node].auto = simple;
            placed[node].outer = simple;
            placed[node].body = if state.kind == NodeKind::Group {
                Rect::ZERO
            } else {
                self.bbox_with_label(state, position, simple.height, simple)
            };
        }
        Ok(self.fill_compounds(placed))
    }

    fn run_layout(
        &self,
        geometry: &[NodeGeometry],
        segments: Option<&[SegmentStyle]>,
    ) -> Result<Vec<FcosePosition>, ArchitectureRenderError> {
        let fcose_nodes: Vec<FcoseNode> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(node, state)| {
                let size = if state.kind == NodeKind::Group {
                    geometry[node].outer
                } else {
                    Size::new(self.icon_size, self.icon_size)
                };
                FcoseNode {
                    id: state.id.clone(),
                    parent_id: state.parent.map(|parent| self.nodes[parent].id.clone()),
                    x: geometry[node].position.x,
                    y: geometry[node].position.y,
                    width: size.width,
                    height: size.height,
                    padding: state.padding.trunc(),
                }
            })
            .collect();

        let fcose_edges: Vec<FcoseEdge> = self
            .edges
            .iter()
            .enumerate()
            .map(|(index, edge)| {
                FcoseEdge::new(
                    format!("edge-{index}"),
                    edge.lhs_id.clone(),
                    edge.rhs_id.clone(),
                )
            })
            .collect();

        let icon_size = self.icon_size;
        let same_group_length = self.config.architecture.ideal_edge_length_multiplier * icon_size;
        let cross_group_length = 0.5 * icon_size;
        let same_group_elasticity = self.config.architecture.edge_elasticity;

        let options = FcoseOptions {
            quality: FcoseQuality::Proof,
            randomize: self.config.architecture.randomize,
            animate: false,
            node_dimensions_include_labels: false,
            node_separation: self.config.architecture.node_separation,
            num_iter: self.config.architecture.num_iter as i64,
            ideal_edge_length: FcoseEdgeValue::Function(Arc::new(move |edge: &FcoseEdgeInput| {
                if edge.source_parent == edge.target_parent {
                    same_group_length
                } else {
                    cross_group_length
                }
            })),
            edge_elasticity: FcoseEdgeValue::Function(Arc::new(move |edge: &FcoseEdgeInput| {
                if edge.source_parent == edge.target_parent {
                    same_group_elasticity
                } else {
                    0.001
                }
            })),
            alignment_constraint: self.alignment_constraint(),
            relative_placement_constraint: self.relative_constraints(),
            seed: self.config.architecture.seed as u32,
            ..Default::default()
        };
        let mut positions = run_fcose_layout(&fcose_nodes, &fcose_edges, &options)?;

        // fcose relocates the result so its center is the center of the graph it read from
        // cytoscape's node positions and sizes. Cytoscape's own bounding box also includes the
        // labels (and the one-point expansion), so a label wider than its node moves the
        // center; shift the result by the difference.
        let port_center = layout_center(&fcose_nodes);
        let full_center = self.graph_center(geometry, segments);
        let delta = (full_center.0 - port_center.0, full_center.1 - port_center.1);
        if delta.0 != 0.0 || delta.1 != 0.0 {
            for position in &mut positions {
                position.x += delta.0;
                position.y += delta.1;
            }
        }
        Ok(positions)
    }

    /// `getAlignments`: the rows and columns the spatial maps imply, flattened per group
    /// alignment, with any heuristic row that contains a declared `align` member dropped.
    fn alignment_constraint(&self) -> Option<AlignmentConstraint> {
        let mut horizontal: Vec<Vec<String>> = Vec::new();
        let mut vertical: Vec<Vec<String>> = Vec::new();

        for spatial_map in &self.data.spatial_maps {
            let mut horizontal_map: AxisAlignments = Vec::new();
            let mut vertical_map: AxisAlignments = Vec::new();

            for (id, (x, y)) in spatial_map {
                let group = self
                    .index
                    .get(id)
                    .map(|&index| &self.nodes[index])
                    .and_then(|state| state.parent)
                    .map(|parent| self.nodes[parent].id.clone())
                    .unwrap_or_else(|| "default".to_string());

                for (map, coordinate) in [(&mut horizontal_map, *y), (&mut vertical_map, *x)] {
                    let entry = match map.iter_mut().find(|(key, _)| *key == coordinate) {
                        Some(entry) => entry,
                        None => {
                            map.push((coordinate, Vec::new()));
                            map.last_mut().expect("just pushed")
                        }
                    };
                    match entry.1.iter_mut().find(|(name, _)| *name == group) {
                        Some((_, ids)) => ids.push(id.clone()),
                        None => entry.1.push((group.clone(), vec![id.clone()])),
                    }
                }
            }

            horizontal.extend(flatten_alignments(
                &horizontal_map,
                Alignment::Horizontal,
                &self.data,
            ));
            vertical.extend(flatten_alignments(
                &vertical_map,
                Alignment::Vertical,
                &self.data,
            ));
        }

        let declared: Vec<&str> = self
            .layout_hints
            .iter()
            .flat_map(|hint| hint.members.iter().map(String::as_str))
            .collect();

        let mut horizontal: Vec<Vec<String>> = horizontal
            .into_iter()
            .filter(|group| {
                group.len() > 1 && !group.iter().any(|id| declared.contains(&id.as_str()))
            })
            .collect();
        let mut vertical: Vec<Vec<String>> = vertical
            .into_iter()
            .filter(|group| {
                group.len() > 1 && !group.iter().any(|id| declared.contains(&id.as_str()))
            })
            .collect();

        for hint in &self.layout_hints {
            if hint.members.len() < 2 {
                continue;
            }
            if hint.direction == AlignmentDirection::Row {
                horizontal.push(hint.members.clone());
            } else {
                vertical.push(hint.members.clone());
            }
        }

        if horizontal.is_empty() && vertical.is_empty() {
            return None;
        }
        Some(AlignmentConstraint {
            vertical: (!vertical.is_empty()).then_some(vertical),
            horizontal: (!horizontal.is_empty()).then_some(horizontal),
        })
    }

    /// `getRelativeConstraints`: a chain per declared `align`, then the BFS over each spatial
    /// map's inverse positions, skipping pairs a declared chain already orders.
    fn relative_constraints(&self) -> Option<Vec<RelativePlacementConstraint>> {
        let mut constraints = Vec::new();
        let mut declared_pairs: Vec<(String, String)> = Vec::new();
        let gap = self.config.architecture.ideal_edge_length_multiplier * self.icon_size;

        for hint in &self.layout_hints {
            for pair in hint.members.windows(2) {
                let (a, b) = (&pair[0], &pair[1]);
                declared_pairs.push((a.clone(), b.clone()));
                declared_pairs.push((b.clone(), a.clone()));
                constraints.push(if hint.direction == AlignmentDirection::Row {
                    RelativePlacementConstraint {
                        left: Some(a.clone()),
                        right: Some(b.clone()),
                        top: None,
                        bottom: None,
                        gap: Some(gap),
                    }
                } else {
                    RelativePlacementConstraint {
                        left: None,
                        right: None,
                        top: Some(a.clone()),
                        bottom: Some(b.clone()),
                        gap: Some(gap),
                    }
                });
            }
        }

        for spatial_map in &self.data.spatial_maps {
            let inverse: Vec<((i32, i32), &str)> = spatial_map
                .iter()
                .map(|(id, position)| (*position, id.as_str()))
                .collect();
            let lookup = |position: (i32, i32)| -> Option<&str> {
                inverse
                    .iter()
                    .rev()
                    .find(|(candidate, _)| *candidate == position)
                    .map(|(_, id)| *id)
            };

            let mut queue: Vec<(i32, i32)> = vec![(0, 0)];
            let mut visited: Vec<(i32, i32)> = Vec::new();
            while !queue.is_empty() {
                let current = queue.remove(0);
                if !visited.contains(&current) {
                    visited.push(current);
                }
                let Some(current_id) = lookup(current) else {
                    continue;
                };
                for (direction, shift) in [
                    (Direction::Left, (-1, 0)),
                    (Direction::Right, (1, 0)),
                    (Direction::Top, (0, 1)),
                    (Direction::Bottom, (0, -1)),
                ] {
                    let neighbour = (current.0 + shift.0, current.1 + shift.1);
                    let Some(neighbour_id) = lookup(neighbour) else {
                        continue;
                    };
                    if visited.contains(&neighbour) {
                        continue;
                    }
                    queue.push(neighbour);
                    if declared_pairs
                        .iter()
                        .any(|(a, b)| a == current_id && b == neighbour_id)
                    {
                        continue;
                    }
                    constraints.push(match direction {
                        Direction::Left => RelativePlacementConstraint {
                            left: Some(neighbour_id.to_string()),
                            right: Some(current_id.to_string()),
                            top: None,
                            bottom: None,
                            gap: Some(gap),
                        },
                        Direction::Right => RelativePlacementConstraint {
                            left: Some(current_id.to_string()),
                            right: Some(neighbour_id.to_string()),
                            top: None,
                            bottom: None,
                            gap: Some(gap),
                        },
                        Direction::Top => RelativePlacementConstraint {
                            left: None,
                            right: None,
                            top: Some(neighbour_id.to_string()),
                            bottom: Some(current_id.to_string()),
                            gap: Some(gap),
                        },
                        Direction::Bottom => RelativePlacementConstraint {
                            left: None,
                            right: None,
                            top: Some(current_id.to_string()),
                            bottom: Some(neighbour_id.to_string()),
                            gap: Some(gap),
                        },
                    });
                }
            }
        }

        (!constraints.is_empty()).then_some(constraints)
    }

    /// `layoutstop`'s segment weights and distances for an XY edge, and fcose's defaults for
    /// every other edge.
    fn edge_segment(&self, edge: &ast::Edge, geometry: &[NodeGeometry]) -> SegmentStyle {
        let default = SegmentStyle {
            weight: SEGMENT_WEIGHT,
            distance: SEGMENT_DISTANCE,
        };
        if Alignment::for_directions(edge.lhs_dir, edge.rhs_dir) != Alignment::Bend {
            return default;
        }
        let source = self.node_endpoint(&edge.lhs_id, edge.lhs_dir, geometry);
        let target = self.node_endpoint(&edge.rhs_id, edge.rhs_dir, geometry);
        let source_position = geometry[self.index[&edge.lhs_id]].position;
        let target_position = geometry[self.index[&edge.rhs_id]].position;
        if source_position.x == target_position.x || source_position.y == target_position.y {
            return default;
        }
        let (point_x, point_y) = if edge.lhs_dir.is_vertical() {
            (source.x, target.y)
        } else {
            (target.x, source.y)
        };
        segment_weights(source, target, point_x, point_y)
    }

    /// The center of cytoscape's `eles.boundingBox()`: the nodes' boxes with labels, plus each
    /// edge's box around its points, half the edge width and the one-point antialiasing
    /// expansion.
    fn graph_center(
        &self,
        geometry: &[NodeGeometry],
        segments: Option<&[SegmentStyle]>,
    ) -> (f64, f64) {
        let mut x1 = f64::INFINITY;
        let mut y1 = f64::INFINITY;
        let mut x2 = f64::NEG_INFINITY;
        let mut y2 = f64::NEG_INFINITY;
        let mut include = |bounds: Rect| {
            x1 = x1.min(bounds.x);
            x2 = x2.max(bounds.right());
            y1 = y1.min(bounds.y);
            y2 = y2.max(bounds.bottom());
        };
        for node in geometry {
            include(node.body);
        }
        let half_width = self.theme_length("archEdgeWidth", 3.0) / 2.0;
        for (index, edge) in self.edges.iter().enumerate() {
            let start = self.node_endpoint(&edge.lhs_id, edge.lhs_dir, geometry);
            let end = self.node_endpoint(&edge.rhs_id, edge.rhs_dir, geometry);
            let segment = segments
                .map(|segments| segments[index])
                .unwrap_or(SegmentStyle {
                    weight: SEGMENT_WEIGHT,
                    distance: SEGMENT_DISTANCE,
                });
            let (mid_x, mid_y) = self.edge_midpoint(edge, start, end, &segment);
            let xs = [start.x, mid_x, end.x];
            let ys = [start.y, mid_y, end.y];
            let min_x = xs.iter().copied().fold(f64::INFINITY, f64::min) - half_width;
            let max_x = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max) + half_width;
            let min_y = ys.iter().copied().fold(f64::INFINITY, f64::min) - half_width;
            let max_y = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max) + half_width;
            let mut bounds = Rect::new(min_x, min_y, max_x - min_x, max_y - min_y);
            if let Some(label) = edge.title.as_deref().filter(|label| !label.is_empty()) {
                let measured = self.label_box(label);
                let label = Rect::new(
                    mid_x - measured.width / 2.0,
                    mid_y - measured.height / 2.0,
                    measured.width,
                    measured.height,
                );
                bounds = bounds.union(label);
            }
            include(bounds.expanded(BBOX_EXPANSION));
        }
        ((x1 + x2) / 2.0, (y1 + y2) / 2.0)
    }

    // ---- drawing ----

    fn draw_edge(
        &self,
        parent: &mut Group,
        edge: &ast::Edge,
        geometry: &[NodeGeometry],
        segment: &SegmentStyle,
    ) {
        let mut group = Group::new().with_source(SourceRef::edge(format!(
            "L_{}_{}_0",
            edge.lhs_id, edge.rhs_id
        )));
        let mut start = self.node_endpoint(&edge.lhs_id, edge.lhs_dir, geometry);
        let mut end = self.node_endpoint(&edge.rhs_id, edge.rhs_dir, geometry);
        let raw_start = start;
        let raw_end = end;

        let half_icon = self.icon_size / 2.0;
        let group_edge_shift = self.padding + 4.0;

        if edge.lhs_group {
            if edge.lhs_dir.is_horizontal() {
                start.x += if edge.lhs_dir == Direction::Left {
                    -group_edge_shift
                } else {
                    group_edge_shift
                };
            } else {
                start.y += if edge.lhs_dir == Direction::Top {
                    -group_edge_shift
                } else {
                    group_edge_shift + GROUP_LABEL_HEIGHT
                };
            }
        }
        if edge.rhs_group {
            if edge.rhs_dir.is_horizontal() {
                end.x += if edge.rhs_dir == Direction::Left {
                    -group_edge_shift
                } else {
                    group_edge_shift
                };
            } else {
                end.y += if edge.rhs_dir == Direction::Top {
                    -group_edge_shift
                } else {
                    group_edge_shift + GROUP_LABEL_HEIGHT
                };
            }
        }
        if !edge.lhs_group && self.node_kind(&edge.lhs_id) == Some(NodeKind::Junction) {
            if edge.lhs_dir.is_horizontal() {
                start.x += if edge.lhs_dir == Direction::Left {
                    half_icon
                } else {
                    -half_icon
                };
            } else {
                start.y += if edge.lhs_dir == Direction::Top {
                    half_icon
                } else {
                    -half_icon
                };
            }
        }
        if !edge.rhs_group && self.node_kind(&edge.rhs_id) == Some(NodeKind::Junction) {
            if edge.rhs_dir.is_horizontal() {
                end.x += if edge.rhs_dir == Direction::Left {
                    half_icon
                } else {
                    -half_icon
                };
            } else {
                end.y += if edge.rhs_dir == Direction::Top {
                    half_icon
                } else {
                    -half_icon
                };
            }
        }

        let midpoint = self.edge_midpoint(edge, raw_start, raw_end, segment);
        let mut path = Path::new();
        path.move_to(start)
            .line_to(Point::new(midpoint.0, midpoint.1))
            .line_to(end);
        group.push(PathShape::new(path, self.edge_style()));

        let arrow_size = self.icon_size / ARROW_SCALE_DIVISOR;
        if edge.lhs_into {
            group.push(self.arrow(edge.lhs_dir, start, arrow_size));
        }
        if edge.rhs_into {
            group.push(self.arrow(edge.rhs_dir, end, arrow_size));
        }

        if let Some(label) = edge.title.as_deref().filter(|label| !label.is_empty()) {
            let axis = if edge.lhs_dir.is_horizontal() != edge.rhs_dir.is_horizontal() {
                Axis::Xy
            } else if edge.lhs_dir.is_horizontal() {
                Axis::X
            } else {
                Axis::Y
            };
            let width = match axis {
                Axis::X => (start.x - end.x).abs(),
                Axis::Y => (start.y - end.y).abs() / 1.5,
                Axis::Xy => (start.x - end.x).abs() / 2.0,
            };
            let mut label_group = self.label_group(label, width, TextAnchor::Middle);
            match axis {
                Axis::X => {
                    label_group.transform = Transform::translate(midpoint.0, midpoint.1);
                }
                Axis::Y => {
                    label_group.transform =
                        Transform::rotate(-90.0).then(Transform::translate(midpoint.0, midpoint.1));
                }
                Axis::Xy => {
                    let pair = ast::DirectionPair::new(edge.lhs_dir, edge.rhs_dir)
                        .expect("a non-identical direction pair");
                    let factors = xy_factors(pair);
                    let block = label_block(self.measure, label, width, &self.font);
                    label_group.transform = Transform::rotate_about(
                        -factors.0 * factors.1 * 45.0,
                        Point::new(0.0, block.height / 2.0),
                    )
                    .then(Transform::translate(
                        factors.0 * block.width / 2.0,
                        factors.1 * block.height / 2.0,
                    ))
                    .then(Transform::translate(
                        midpoint.0,
                        midpoint.1 - block.height / 2.0,
                    ));
                }
            }
            group.push(label_group);
        }
        parent.push(group);
    }

    fn edge_midpoint(
        &self,
        edge: &ast::Edge,
        source: Point,
        target: Point,
        segment: &SegmentStyle,
    ) -> (f64, f64) {
        if edge.lhs_dir.is_horizontal() != edge.rhs_dir.is_horizontal() {
            let dx = target.x - source.x;
            let dy = target.y - source.y;
            let length = (dx * dx + dy * dy).sqrt();
            let inverse = if length == 0.0 {
                (0.0, 0.0)
            } else {
                (-dy / length, dx / length)
            };
            let weight1 = 1.0 - segment.weight;
            let adjusted = (
                source.x * weight1 + target.x * segment.weight,
                source.y * weight1 + target.y * segment.weight,
            );
            return (
                adjusted.0 + inverse.0 * segment.distance,
                adjusted.1 + inverse.1 * segment.distance,
            );
        }
        // A straight edge's midpoint includes both arrow endpoints, which are the endpoints
        // themselves because the edges carry no cytoscape arrow shape.
        (
            (source.x + target.x + source.x + target.x) / 4.0,
            (source.y + target.y + source.y + target.y) / 4.0,
        )
    }

    fn arrow(&self, direction: Direction, at: Point, size: f64) -> Group {
        let points = match direction {
            Direction::Left => vec![
                Point::new(size, size / 2.0),
                Point::new(0.0, size),
                Point::new(0.0, 0.0),
            ],
            Direction::Right => vec![
                Point::new(0.0, size / 2.0),
                Point::new(size, 0.0),
                Point::new(size, size),
            ],
            Direction::Top => vec![
                Point::new(0.0, 0.0),
                Point::new(size, 0.0),
                Point::new(size / 2.0, size),
            ],
            Direction::Bottom => vec![
                Point::new(size / 2.0, 0.0),
                Point::new(size, size),
                Point::new(0.0, size),
            ],
        };
        let half = size / 2.0;
        let (x, y) = if direction.is_horizontal() {
            (
                match direction {
                    Direction::Left => at.x - size + 2.0,
                    _ => at.x - 2.0,
                },
                at.y - half,
            )
        } else {
            (
                at.x - half,
                match direction {
                    Direction::Top => at.y - size + 2.0,
                    _ => at.y - 2.0,
                },
            )
        };
        let mut group = Group::translated(x, y);
        group.push(PathShape::new(
            Path::polygon(points),
            PathStyle::filled(self.theme_color("archEdgeArrowColor", "#333333")),
        ));
        group
    }

    fn draw_service(&self, parent: &mut Group, state: &NodeState, geometry: &NodeGeometry) {
        let Some(service) = self.services.get(&state.id) else {
            return;
        };
        let mut group = Group::translated(geometry.position.x, geometry.position.y)
            .with_source(SourceRef::node(service.id.clone()));
        group.class = Some("architecture-service".to_string());

        if let Some(title) = service.title.as_deref().filter(|title| !title.is_empty()) {
            let mut label = self.label_group(title, self.icon_size * 1.5, TextAnchor::Middle);
            label.transform = Transform::translate(self.icon_size / 2.0, self.icon_size);
            group.push(label);
        }

        if let Some(icon) = service.icon.as_deref().filter(|icon| !icon.is_empty()) {
            group.push(icons::icon_group(icon, self.icon_size, self.icon_size));
        } else if let Some(text) = service.icon_text.as_deref().filter(|text| !text.is_empty()) {
            group.push(icons::icon_group("blank", self.icon_size, self.icon_size));
            group.push(self.icon_text_group(text));
        } else {
            let size = self.icon_size;
            let corner = NODE_BKG_CORNER;
            let mut path = Path::new();
            path.move_to(Point::new(0.0, size))
                .line_to(Point::new(0.0, corner))
                .quad_to(Point::new(0.0, 0.0), Point::new(corner, 0.0))
                .line_to(Point::new(size - corner, 0.0))
                .quad_to(Point::new(size, 0.0), Point::new(size, corner))
                .line_to(Point::new(size, size))
                .close();
            group.push(PathShape::new(path, self.node_bkg_style()));
        }
        parent.push(group);
    }

    fn draw_junction(&self, parent: &mut Group, state: &NodeState, geometry: &NodeGeometry) {
        let mut group = Group::translated(geometry.position.x, geometry.position.y)
            .with_source(SourceRef::node(state.id.clone()));
        group.class = Some("architecture-junction".to_string());
        let mut bkg = Group::new();
        bkg.push(RectShape::new(
            Rect::new(0.0, 0.0, self.icon_size, self.icon_size),
            0.0,
            PathStyle {
                fill: None,
                stroke: None,
                ..PathStyle::default()
            },
        ));
        group.push(bkg);
        parent.push(group);
    }

    fn draw_group(
        &self,
        parent: &mut Group,
        node: usize,
        state: &NodeState,
        geometry: &NodeGeometry,
    ) {
        let bbox = geometry.body;
        let half = self.icon_size / 2.0;
        let mut rect = RectShape::new(
            Rect::new(bbox.x + half, bbox.y + half, bbox.width, bbox.height),
            0.0,
            self.node_bkg_style(),
        );
        rect.source = Some(SourceRef::cluster(state.id.clone()));
        parent.push(rect);

        let mut label_group = Group::new();
        let mut shifted_x = bbox.x;
        let mut shifted_y = bbox.y;
        let group_icon_size = self.padding * 0.75;
        if let Some(icon) = self.group_icon(node) {
            let mut wrapper = Group::translated(shifted_x + half + 1.0, shifted_y + half + 1.0);
            wrapper.push(icons::icon_group(icon, group_icon_size, group_icon_size));
            label_group.push(wrapper);
            shifted_x += group_icon_size;
            shifted_y += self.label_font_size / 2.0 - 1.0 - 2.0;
        }
        if let Some(title) = state.label.as_deref().filter(|title| !title.is_empty()) {
            let mut label = self.label_group(title, bbox.width, TextAnchor::Start);
            label.transform = Transform::translate(shifted_x + half + 4.0, shifted_y + half + 2.0);
            label_group.push(label);
        }
        parent.push(label_group);
    }

    fn group_icon(&self, node: usize) -> Option<&str> {
        self.group_icons
            .get(&self.nodes[node].id)
            .and_then(|icon| icon.as_deref())
            .filter(|icon| !icon.is_empty())
    }

    fn icon_text_group(&self, text: &str) -> Group {
        let width = (self.icon_size - 2.0).max(1.0);
        let block = shape_text(
            text,
            &TextOptions {
                width,
                markdown: false,
                min_width: 0.0,
            },
            &self.font,
            self.measure,
        )
        .unwrap_or_else(|_| plain_text_block(text, &self.font, self.measure));
        let mut run = TextRun::new(
            Point::new(self.icon_size / 2.0, self.icon_size / 2.0),
            self.font.clone(),
            block,
        );
        run.anchor = TextAnchor::Middle;
        run.baseline = TextBaseline::Middle;
        run.fill = Some(Color::new("#fff"));
        let mut group = Group::new();
        group.push(run);
        group
    }

    /// `createText`'s SVG label: a group the caller positions, with the first baseline
    /// `font.size - 10.1` below its origin and the measured block under it.
    fn label_group(&self, text: &str, width: f64, anchor: TextAnchor) -> Group {
        let block = shape_text(
            text,
            &TextOptions {
                width,
                markdown: false,
                min_width: 0.0,
            },
            &self.font,
            self.measure,
        )
        .unwrap_or_else(|_| plain_text_block(text, &self.font, self.measure));
        let baseline = LINE_HEIGHT_EM * self.font.size - 0.1 * self.font.size + TEXT_Y;
        let mut run = TextRun::new(Point::new(0.0, baseline), self.font.clone(), block);
        run.anchor = anchor;
        run.fill = Some(self.theme_color("textColor", "#333333"));
        let mut group = Group::new();
        group.push(run);
        group
    }

    fn theme_color(&self, key: &str, fallback: &str) -> Color {
        Color::new(
            self.config
                .theme_variables
                .get(key)
                .and_then(|value| value.as_str())
                .unwrap_or(fallback),
        )
    }

    fn theme_length(&self, key: &str, fallback: f64) -> f64 {
        self.config
            .theme_variables
            .get(key)
            .and_then(|value| value.as_str())
            .map(|value| value.trim().trim_end_matches("px"))
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(fallback)
    }

    fn edge_style(&self) -> PathStyle {
        PathStyle {
            fill: None,
            stroke: Some(self.theme_color("archEdgeColor", "#333333")),
            stroke_width: self.theme_length("archEdgeWidth", 3.0),
            ..PathStyle::default()
        }
    }

    fn node_bkg_style(&self) -> PathStyle {
        PathStyle {
            fill: None,
            stroke: Some(self.theme_color("archGroupBorderColor", "#000000")),
            stroke_width: self.theme_length("archGroupBorderWidth", 2.0),
            dash: vec![GROUP_DASH],
            ..PathStyle::default()
        }
    }
}

/// The center of the layout graph `run_fcose_layout` recenters to, from the node sizes it is
/// given.
fn layout_center(nodes: &[FcoseNode]) -> (f64, f64) {
    let mut x1 = f64::INFINITY;
    let mut y1 = f64::INFINITY;
    let mut x2 = f64::NEG_INFINITY;
    let mut y2 = f64::NEG_INFINITY;
    for node in nodes {
        x1 = x1.min(node.x - node.width / 2.0);
        x2 = x2.max(node.x + node.width / 2.0);
        y1 = y1.min(node.y - node.height / 2.0);
        y2 = y2.max(node.y + node.height / 2.0);
    }
    ((x1 + x2) / 2.0, (y1 + y2) / 2.0)
}

/// `getSegmentWeights`, the bend `layoutstop` writes into `segment-weights`/`-distances`.
fn segment_weights(source: Point, target: Point, point_x: f64, point_y: f64) -> SegmentStyle {
    let (sx, sy) = (source.x, source.y);
    let (tx, ty) = (target.x, target.y);
    if sx == tx {
        return SegmentStyle {
            weight: SEGMENT_WEIGHT,
            distance: SEGMENT_DISTANCE,
        };
    }
    let mut distance = (point_y - sy + ((sx - point_x) * (sy - ty)) / (sx - tx))
        / (1.0 + ((sy - ty) / (sx - tx)).powi(2)).sqrt();
    let mut weight = ((point_y - sy).powi(2) + (point_x - sx).powi(2) - distance.powi(2)).sqrt();
    let dist_ab = ((tx - sx).powi(2) + (ty - sy).powi(2)).sqrt();
    weight /= dist_ab;
    let delta1 = (tx - sx) * (point_y - sy) - (ty - sy) * (point_x - sx);
    let delta1 = if delta1 >= 0.0 { 1.0 } else { -1.0 };
    let delta2 = (tx - sx) * (point_x - sx) + (ty - sy) * (point_y - sy);
    let delta2 = if delta2 >= 0.0 { 1.0 } else { -1.0 };
    distance = distance.abs() * delta1;
    weight *= delta2;
    SegmentStyle { weight, distance }
}

#[derive(Clone, Copy, Debug)]
struct SegmentStyle {
    weight: f64,
    distance: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    X,
    Y,
    Xy,
}

/// `getArchitectureDirectionXYFactors`.
fn xy_factors(pair: ast::DirectionPair) -> (f64, f64) {
    match (pair.source, pair.target) {
        (Direction::Left, Direction::Top) | (Direction::Top, Direction::Left) => (1.0, 1.0),
        (Direction::Bottom, Direction::Left) | (Direction::Left, Direction::Bottom) => (1.0, -1.0),
        (Direction::Bottom, Direction::Right) | (Direction::Right, Direction::Bottom) => {
            (-1.0, -1.0)
        }
        _ => (-1.0, 1.0),
    }
}

fn label_block(measure: &dyn TextMeasure, text: &str, width: f64, font: &Font) -> Size {
    shape_text(
        text,
        &TextOptions {
            width,
            markdown: false,
            min_width: 0.0,
        },
        font,
        measure,
    )
    .map(|block| block.size)
    .unwrap_or(Size::ZERO)
}

/// `flattenAlignments` for one axis of one spatial map.
fn flatten_alignments(
    alignment_map: &AxisAlignments,
    alignment: Alignment,
    data: &DataStructures,
) -> Vec<Vec<String>> {
    let mut flattened: Vec<(String, Vec<String>)> = Vec::new();
    for (numeric, alignments) in alignment_map {
        let dir = numeric.to_string();
        if alignments.len() == 1 {
            flattened.push((dir, alignments[0].1.clone()));
            continue;
        }
        let mut count = 0;
        for i in 0..alignments.len().saturating_sub(1) {
            for j in i + 1..alignments.len() {
                let (a_group, a_ids) = &alignments[i];
                let (b_group, b_ids) = &alignments[j];
                let group_alignment = data.group_alignment(a_group, b_group);
                if group_alignment == Some(alignment)
                    || a_group == "default"
                    || b_group == "default"
                {
                    let entry = match flattened.iter_mut().find(|(key, _)| *key == dir) {
                        Some(entry) => entry,
                        None => {
                            flattened.push((dir.clone(), Vec::new()));
                            flattened.last_mut().expect("just pushed")
                        }
                    };
                    entry.1.extend(a_ids.iter().cloned());
                    entry.1.extend(b_ids.iter().cloned());
                } else {
                    let key_a = format!("{dir}-{}", count);
                    count += 1;
                    flattened.push((key_a, a_ids.clone()));
                    let key_b = format!("{dir}-{}", count);
                    count += 1;
                    flattened.push((key_b, b_ids.clone()));
                }
            }
        }
    }
    flattened.into_iter().map(|(_, ids)| ids).collect()
}

/// The box of everything painted, with group transforms applied, matching the `getBBox()`
/// that `setupGraphviewbox` fits the viewBox to.
fn scene_bounds(items: &[Item]) -> Option<Rect> {
    fn include(out: &mut Option<Rect>, rect: Rect) {
        *out = Some(out.map_or(rect, |bounds| bounds.union(rect)));
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
                if let Some(bounds) = crate::render::pipeline::dagre::svg_path_bounds(&shape.path) {
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
            Item::Text(text) => include(out, transformed(text.bounds(), transform)),
            Item::Image(image) => include(out, transformed(image.rect, transform)),
        }
    }

    let mut bounds = None;
    for item in items {
        walk(item, Transform::IDENTITY, &mut bounds);
    }
    bounds
}
