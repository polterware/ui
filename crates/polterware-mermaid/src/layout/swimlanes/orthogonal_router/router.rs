//! Port of upstream mermaid 12.3.0
//! `rendering-util/layout-algorithms/swimlanes/orthogonalRouter/router.ts`.
//!
//! Each edge is routed as an axis-aligned (Manhattan) polyline between ports on the node
//! borders, with port distribution and nudging to reduce overlaps and crossings. The approach
//! follows Wybrow, Marriott & Stuckey, "Orthogonal Connector Routing" (the libavoid family).
//!
//! The arithmetic keeps upstream's `f64` order and its map insertion orders: edges are visited
//! in the order the diagram declares them, groups of siblings in the order their first edge
//! appeared, and pipes in creation order.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::layout::swimlanes::anchored_nodes::collect_anchored_ids;
use crate::layout::swimlanes::config::PRECISION;
use crate::layout::swimlanes::helpers::Direction;
use crate::layout::swimlanes::lanes::build_lane_model;
use crate::render::pipeline::types::{Edge, LayoutData, Node};
use crate::scene::Point;

const EPS: f64 = PRECISION.epsilon;

const NODE_PADDING: f64 = 8.0;
const HORIZONTAL_PIPE_MARGIN: f64 = 15.0;
const VERTICAL_PIPE_MARGIN: f64 = 15.0;
const ROUTING_MARGIN: f64 = 25.0;
const ANCHOR_OFFSET: f64 = 20.0;
const TRACK_SPACING: f64 = 10.0;
const CROSSING_PENALTY: f64 = 1000.0;
const MIN_PORT_SPACING: f64 = 8.0;
const MAX_PORT_SPACING: f64 = 20.0;
const MAX_ITERATIONS: usize = 10;

/// One of the four cardinal faces of a node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum OrthogonalSide {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

impl OrthogonalSide {
    fn as_str(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    fn is_vertical(self) -> bool {
        matches!(self, Self::Top | Self::Bottom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Orientation {
    Horizontal,
    Vertical,
}

impl Orientation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SegmentRef {
    edge_index: usize,
    segment_index: usize,
    from: f64,
    to: f64,
}

#[derive(Clone, Debug, PartialEq)]
struct Track {
    index: usize,
    coord: f64,
    segments: Vec<SegmentRef>,
}

#[derive(Clone, Debug, PartialEq)]
struct Pipe {
    id: String,
    orientation: Orientation,
    /// y for a horizontal pipe, x for a vertical one.
    coord: f64,
    span_min: f64,
    span_max: f64,
    tracks: Vec<Track>,
}

/// A routed segment. `pipe` indexes [`Router::pipes`]; it is `None` for the centered
/// straight-line fast path, whose private pipe upstream never registers (so no other edge can
/// snap to it), but whose coordinate the crossing checks still read.
#[derive(Clone, Debug)]
struct RoutedSegment {
    edge_index: usize,
    segment_index: usize,
    orientation: Orientation,
    pipe: Option<usize>,
    pipe_coord: f64,
    track_index: usize,
    from: f64,
    to: f64,
}

#[derive(Clone, Debug)]
struct ObstacleRect {
    node_id: String,
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    /// For LR direction, TB x becomes LR y, so the visual Y extent uses the height.
    visual_x_half_extent: f64,
}

#[derive(Clone, Copy, Debug)]
struct RoutedLine {
    orient: Orientation,
    coord: f64,
    from: f64,
    to: f64,
}

#[derive(Clone, Copy, Debug, Default)]
struct DestInfo {
    dest: f64,
    deviation: f64,
    delta: f64,
}

#[derive(Clone, Debug)]
struct EdgeSideInfo {
    edge_index: usize,
    src_id: String,
    dst_id: String,
    src_side: OrthogonalSide,
    dst_side: OrthogonalSide,
    abs_dx: f64,
    abs_dy: f64,
    dx_sign: f64,
    dy_sign: f64,
}

/// One segment of a pipe as the rebuild phase walks it: who owns it, which track it sits on,
/// and the span it covers.
#[derive(Clone, Copy, Debug)]
struct PipeSegmentInfo {
    edge_index: usize,
    segment_index: usize,
    track_index: usize,
    from: f64,
    to: f64,
}

#[derive(Clone, Copy, Debug)]
struct PortGroupEntry {
    edge_index: usize,
    opposite_coord: f64,
}

#[derive(Clone, Debug)]
struct OpenEntry {
    key: String,
    f: f64,
    pt: Point,
}

#[derive(Clone, Copy, Debug)]
struct Detour {
    x: f64,
    y: f64,
    leaves_positive_side: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MoveMode {
    Segment,
    Chain,
}

/// `Math.sign`: `0` for either zero, unlike `f64::signum`.
fn js_sign(value: f64) -> f64 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// `Number.prototype.toFixed`, used for the string keys upstream builds from coordinates.
fn to_fixed(value: f64, digits: usize) -> String {
    let factor = 10f64.powi(digits as i32);
    let scaled = value * factor;
    let rounded = if scaled >= 0.0 {
        (scaled + 0.5).floor()
    } else {
        (scaled - 0.5).ceil()
    };
    let value = if rounded == 0.0 {
        0.0
    } else {
        rounded / factor
    };
    format!("{value:.digits$}")
}

fn is_drawable_size(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite() && *value > 0.0)
}

/// The size of what is drawn for a node: `metadata.drawnExtent` when a shape draws smaller
/// than its layout box, else the box itself.
fn drawn_size(node: &Node, fallback: f64) -> (f64, f64) {
    let drawn = node
        .metadata
        .as_ref()
        .and_then(|value| value.get("drawnExtent"));
    let width = drawn
        .and_then(|value| value.get("width"))
        .and_then(|value| value.as_f64());
    let height = drawn
        .and_then(|value| value.get("height"))
        .and_then(|value| value.as_f64());
    (
        is_drawable_size(width).unwrap_or_else(|| node.width.unwrap_or(fallback)),
        is_drawable_size(height).unwrap_or_else(|| node.height.unwrap_or(fallback)),
    )
}

fn choose_orthogonal_side(node: &Node, target: Point, fallback: OrthogonalSide) -> OrthogonalSide {
    let cx = node.x.unwrap_or(0.0);
    let cy = node.y.unwrap_or(0.0);
    let dx = target.x - cx;
    let dy = target.y - cy;
    let abs_dx = dx.abs();
    let abs_dy = dy.abs();

    if abs_dx < EPS && abs_dy < EPS {
        return fallback;
    }

    const VERTICAL_BIAS: f64 = 3.0;
    if abs_dy > EPS && abs_dy * VERTICAL_BIAS >= abs_dx {
        return if dy > 0.0 {
            OrthogonalSide::Bottom
        } else {
            OrthogonalSide::Top
        };
    }
    if abs_dx > EPS {
        return if dx > 0.0 {
            OrthogonalSide::Right
        } else {
            OrthogonalSide::Left
        };
    }
    fallback
}

fn shared_line_endpoint_coord(line: RoutedLine, next_line: RoutedLine) -> f64 {
    if (line.to - next_line.from).abs() < EPS || (line.to - next_line.to).abs() < EPS {
        line.to
    } else {
        line.from
    }
}

fn point_on_line(line: RoutedLine, along: f64) -> Point {
    if line.orient == Orientation::Vertical {
        Point::new(line.coord, along)
    } else {
        Point::new(along, line.coord)
    }
}

fn segments_overlap(s1: &SegmentRef, s2: &SegmentRef) -> bool {
    s1.from < s2.to && s2.from < s1.to
}

fn node_is_group(node: &Node) -> bool {
    node.is_group.unwrap_or(false)
}

fn node_shape(node: Option<&Node>) -> Option<&str> {
    node.and_then(|node| node.shape.as_deref())
}

fn push_ordered_group(
    groups: &mut Vec<Vec<usize>>,
    index: &mut HashMap<String, usize>,
    key: &str,
    value: usize,
) {
    match index.get(key) {
        Some(&position) => groups[position].push(value),
        None => {
            index.insert(key.to_string(), groups.len());
            groups.push(vec![value]);
        }
    }
}

struct Router {
    edges: Vec<Edge>,
    original_indices: Vec<usize>,
    node_by_id: HashMap<String, Node>,
    lane_by_node_id: HashMap<String, String>,
    pipes: Vec<Pipe>,
    is_lr: bool,
    obstacles: Vec<ObstacleRect>,
    all_routed_segments: Vec<RoutedSegment>,
    edge_segment_indices: Vec<Vec<usize>>,
    straight_intra_lane_edges: HashSet<usize>,
    side_info_by_idx: BTreeMap<usize, EdgeSideInfo>,
    port_groups: HashMap<String, Vec<PortGroupEntry>>,
    port_offsets: HashMap<String, f64>,
    incident_edge_totals: HashMap<String, usize>,
}

impl Router {
    fn new(data: &LayoutData, direction: Direction) -> Self {
        let nodes = data.nodes.clone();
        let is_lr = matches!(direction, Direction::Lr);

        let mut edges = Vec::new();
        let mut original_indices = Vec::new();
        for (index, edge) in data.edges.iter().enumerate() {
            if edge.is_layout_only.unwrap_or(false) {
                continue;
            }
            edges.push(edge.clone());
            original_indices.push(index);
        }

        let node_by_id: HashMap<String, Node> = nodes
            .iter()
            .map(|node| (node.id.clone(), node.clone()))
            .collect();

        let lane_model = build_lane_model(&nodes);
        let mut lane_by_node_id: HashMap<String, String> = HashMap::new();
        for group in nodes.iter().filter(|node| lane_model.is_lane(&node.id)) {
            let lane_id = group.id.clone();
            assign_lane(&nodes, &mut lane_by_node_id, group, &lane_id);
        }

        let anchored_ids = collect_anchored_ids(&nodes);
        let obstacles: Vec<ObstacleRect> = nodes
            .iter()
            .filter(|node| {
                !node_is_group(node)
                    && !node.is_edge_label.unwrap_or(false)
                    && !anchored_ids.contains(&node.id)
            })
            .map(|node| {
                let width = node.width.unwrap_or(10.0);
                let height = node.height.unwrap_or(10.0);
                let x = node.x.unwrap_or(0.0);
                let y = node.y.unwrap_or(0.0);
                let padding = NODE_PADDING;
                ObstacleRect {
                    node_id: node.id.clone(),
                    min_x: x - width / 2.0 - padding,
                    max_x: x + width / 2.0 + padding,
                    min_y: y - height / 2.0 - padding,
                    max_y: y + height / 2.0 + padding,
                    visual_x_half_extent: if is_lr {
                        height / 2.0 + padding
                    } else {
                        width / 2.0 + padding
                    },
                }
            })
            .collect();

        let segment_count = edges.len();
        Self {
            edges,
            original_indices,
            node_by_id,
            lane_by_node_id,
            pipes: Vec::new(),
            is_lr,
            obstacles,
            all_routed_segments: Vec::new(),
            edge_segment_indices: vec![Vec::new(); segment_count],
            straight_intra_lane_edges: HashSet::new(),
            side_info_by_idx: BTreeMap::new(),
            port_groups: HashMap::new(),
            port_offsets: HashMap::new(),
            incident_edge_totals: HashMap::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Lookups and helpers
    // -----------------------------------------------------------------------

    fn node(&self, id: &str) -> Option<&Node> {
        self.node_by_id.get(id)
    }

    fn get_or_add_pipe(
        &mut self,
        orientation: Orientation,
        coord: f64,
        span_min: f64,
        span_max: f64,
    ) -> usize {
        let existing = self
            .pipes
            .iter()
            .position(|pipe| pipe.orientation == orientation && (pipe.coord - coord).abs() < 1.0);
        let index = match existing {
            Some(index) => index,
            None => {
                self.pipes.push(Pipe {
                    id: format!("pipe-{}-{}", orientation.as_str(), to_fixed(coord, 0)),
                    orientation,
                    coord,
                    span_min,
                    span_max,
                    tracks: Vec::new(),
                });
                self.pipes.len() - 1
            }
        };
        let pipe = &mut self.pipes[index];
        pipe.span_min = pipe.span_min.min(span_min);
        pipe.span_max = pipe.span_max.max(span_max);
        index
    }

    fn port_for_side(node: &Node, side: OrthogonalSide) -> Point {
        let (width, height) = drawn_size(node, 10.0);
        let cx = node.x.unwrap_or(0.0);
        let cy = node.y.unwrap_or(0.0);
        match side {
            OrthogonalSide::Top => Point::new(cx, cy - height / 2.0),
            OrthogonalSide::Bottom => Point::new(cx, cy + height / 2.0),
            OrthogonalSide::Left => Point::new(cx - width / 2.0, cy),
            OrthogonalSide::Right => Point::new(cx + width / 2.0, cy),
        }
    }

    fn get_orthogonal_port(node: &Node, target: Point, is_source: bool) -> Point {
        Self::port_for_side(
            node,
            choose_orthogonal_side(
                node,
                target,
                if is_source {
                    OrthogonalSide::Bottom
                } else {
                    OrthogonalSide::Top
                },
            ),
        )
    }

    fn is_segment_blocked(
        &self,
        p1: Point,
        p2: Point,
        exclude_start: Option<&str>,
        exclude_end: Option<&str>,
    ) -> bool {
        let seg_min_x = p1.x.min(p2.x);
        let seg_max_x = p1.x.max(p2.x);
        let seg_min_y = p1.y.min(p2.y);
        let seg_max_y = p1.y.max(p2.y);

        self.obstacles.iter().any(|obstacle| {
            if exclude_start.is_some_and(|id| obstacle.node_id == id) {
                return false;
            }
            if exclude_end.is_some_and(|id| obstacle.node_id == id) {
                return false;
            }
            if (p1.x - p2.x).abs() > EPS {
                obstacle.min_y < p1.y
                    && obstacle.max_y > p1.y
                    && obstacle.max_x > seg_min_x
                    && obstacle.min_x < seg_max_x
            } else {
                obstacle.min_x < p1.x
                    && obstacle.max_x > p1.x
                    && obstacle.max_y > seg_min_y
                    && obstacle.min_y < seg_max_y
            }
        })
    }

    fn crossing_penalty(&self, edge_index: usize, from: Point, to: Point) -> f64 {
        if self.all_routed_segments.is_empty() {
            return 0.0;
        }
        let is_horizontal = (from.y - to.y).abs() < EPS;
        let is_vertical = (from.x - to.x).abs() < EPS;
        if !is_horizontal && !is_vertical {
            return 0.0;
        }

        let mut penalties = 0.0;
        if is_horizontal {
            let y = from.y;
            let min_x = from.x.min(to.x) - EPS;
            let max_x = from.x.max(to.x) + EPS;
            if max_x <= min_x {
                return 0.0;
            }
            for segment in &self.all_routed_segments {
                if segment.edge_index == edge_index || segment.orientation != Orientation::Vertical
                {
                    continue;
                }
                if segment.pipe_coord < min_x || segment.pipe_coord > max_x {
                    continue;
                }
                if segment.from - EPS <= y && segment.to + EPS >= y {
                    penalties += CROSSING_PENALTY;
                }
            }
        } else {
            let x = from.x;
            let min_y = from.y.min(to.y) - EPS;
            let max_y = from.y.max(to.y) + EPS;
            if max_y <= min_y {
                return 0.0;
            }
            for segment in &self.all_routed_segments {
                if segment.edge_index == edge_index
                    || segment.orientation != Orientation::Horizontal
                {
                    continue;
                }
                if segment.pipe_coord < min_y || segment.pipe_coord > max_y {
                    continue;
                }
                if segment.from - EPS <= x && segment.to + EPS >= x {
                    penalties += CROSSING_PENALTY;
                }
            }
        }
        penalties
    }

    fn routing_order(&self) -> Vec<usize> {
        let mut order: Vec<(usize, i32, f64, f64)> = self
            .edges
            .iter()
            .enumerate()
            .map(|(index, edge)| {
                let (Some(start), Some(end)) = (&edge.start, &edge.end) else {
                    return (index, 0, 0.0, 0.0);
                };
                let src_node = self.node(start);
                let dst_node = self.node(end);
                let src_lane = self.lane_by_node_id.get(start);
                let dst_lane = self.lane_by_node_id.get(end);
                let cross_lane = match (src_lane, dst_lane) {
                    (Some(src), Some(dst)) if src != dst => 1,
                    _ => 0,
                };
                let dx = match (src_node, dst_node) {
                    (Some(src), Some(dst)) => (dst.x.unwrap_or(0.0) - src.x.unwrap_or(0.0)).abs(),
                    _ => 0.0,
                };
                let dy = match (src_node, dst_node) {
                    (Some(src), Some(dst)) => (dst.y.unwrap_or(0.0) - src.y.unwrap_or(0.0)).abs(),
                    _ => 0.0,
                };
                (index, cross_lane, dx, dy)
            })
            .collect();

        order.sort_by(|a, b| {
            if a.1 != b.1 {
                return b.1.cmp(&a.1);
            }
            let a_dist = a.2 + a.3;
            let b_dist = b.2 + b.3;
            if (a_dist - b_dist).abs() > 1.0 {
                return a_dist
                    .partial_cmp(&b_dist)
                    .unwrap_or(std::cmp::Ordering::Equal);
            }
            a.0.cmp(&b.0)
        });
        order.into_iter().map(|entry| entry.0).collect()
    }

    // -----------------------------------------------------------------------
    // Step 6: port pre-assignment
    // -----------------------------------------------------------------------

    fn determine_side(node: &Node, target: Point) -> OrthogonalSide {
        choose_orthogonal_side(node, target, OrthogonalSide::Bottom)
    }

    fn build_side_info(&mut self) {
        for index in 0..self.edges.len() {
            let edge = &self.edges[index];
            let (Some(start), Some(end)) = (edge.start.clone(), edge.end.clone()) else {
                continue;
            };
            if start == end {
                continue;
            }
            if edge
                .points
                .as_ref()
                .is_some_and(|points| !points.is_empty())
            {
                continue;
            }
            let (Some(src), Some(dst)) = (self.node(&start), self.node(&end)) else {
                continue;
            };
            let dx = dst.x.unwrap_or(0.0) - src.x.unwrap_or(0.0);
            let dy = dst.y.unwrap_or(0.0) - src.y.unwrap_or(0.0);
            let src_target = Point::new(dst.x.unwrap_or(0.0), dst.y.unwrap_or(0.0));
            let dst_target = Point::new(src.x.unwrap_or(0.0), src.y.unwrap_or(0.0));
            let info = EdgeSideInfo {
                edge_index: index,
                src_id: start,
                dst_id: end,
                src_side: Self::determine_side(src, src_target),
                dst_side: Self::determine_side(dst, dst_target),
                abs_dx: dx.abs(),
                abs_dy: dy.abs(),
                dx_sign: js_sign(dx),
                dy_sign: js_sign(dy),
            };
            self.side_info_by_idx.insert(index, info);
        }
    }

    fn preference_strength(info: &EdgeSideInfo) -> f64 {
        if info.src_side.is_vertical() {
            if info.abs_dx == 0.0 {
                f64::INFINITY
            } else {
                info.abs_dy / info.abs_dx
            }
        } else if info.abs_dy == 0.0 {
            f64::INFINITY
        } else {
            info.abs_dx / info.abs_dy
        }
    }

    fn secondary_sides(info: &EdgeSideInfo) -> Vec<OrthogonalSide> {
        if info.src_side.is_vertical() {
            if info.dx_sign == 0.0 {
                return vec![OrthogonalSide::Right, OrthogonalSide::Left];
            }
            return vec![if info.dx_sign > 0.0 {
                OrthogonalSide::Right
            } else {
                OrthogonalSide::Left
            }];
        }
        if info.dy_sign == 0.0 {
            return vec![OrthogonalSide::Bottom, OrthogonalSide::Top];
        }
        vec![if info.dy_sign > 0.0 {
            OrthogonalSide::Bottom
        } else {
            OrthogonalSide::Top
        }]
    }

    fn secondary_side(info: &EdgeSideInfo) -> OrthogonalSide {
        Self::secondary_sides(info)[0]
    }

    fn load_key(node_id: &str, side: OrthogonalSide) -> String {
        format!("{node_id}:{}", side.as_str())
    }

    /// Step 6.2: sibling side-splitting (diss.pdf §6.1.2.2).
    fn split_sibling_sides(&mut self) {
        let infos: Vec<EdgeSideInfo> = self.side_info_by_idx.values().cloned().collect();

        let mut source_side_groups: Vec<Vec<usize>> = Vec::new();
        let mut group_index: HashMap<String, usize> = HashMap::new();
        for info in &infos {
            let key = format!("{}:{}", info.src_id, info.src_side.as_str());
            push_ordered_group(
                &mut source_side_groups,
                &mut group_index,
                &key,
                info.edge_index,
            );
        }

        let mut side_load: HashMap<String, i64> = HashMap::new();
        for info in &infos {
            *side_load
                .entry(Self::load_key(&info.src_id, info.src_side))
                .or_insert(0) += 1;
            *side_load
                .entry(Self::load_key(&info.dst_id, info.dst_side))
                .or_insert(0) += 1;
        }

        for group in &mut source_side_groups {
            if group.len() < 2 {
                continue;
            }
            group.sort_by(|&a, &b| {
                let strength_a = Self::preference_strength(&self.side_info_by_idx[&a]);
                let strength_b = Self::preference_strength(&self.side_info_by_idx[&b]);
                if (strength_a - strength_b).abs() > 1e-9 {
                    return strength_b
                        .partial_cmp(&strength_a)
                        .unwrap_or(std::cmp::Ordering::Equal);
                }
                a.cmp(&b)
            });

            for &edge_index in &group[1..] {
                let info = self.side_info_by_idx[&edge_index].clone();
                let primary_load = *side_load
                    .get(&Self::load_key(&info.src_id, info.src_side))
                    .unwrap_or(&0);

                let candidates = Self::secondary_sides(&info);
                let mut secondary = candidates[0];
                for &side in &candidates[1..] {
                    let side_load_value = *side_load
                        .get(&Self::load_key(&info.src_id, side))
                        .unwrap_or(&0);
                    let best_load = *side_load
                        .get(&Self::load_key(&info.src_id, secondary))
                        .unwrap_or(&0);
                    if side_load_value < best_load {
                        secondary = side;
                    }
                }
                let secondary_load = *side_load
                    .get(&Self::load_key(&info.src_id, secondary))
                    .unwrap_or(&0);
                if secondary_load >= primary_load {
                    continue;
                }

                side_load.insert(
                    Self::load_key(&info.src_id, info.src_side),
                    primary_load - 1,
                );
                side_load.insert(Self::load_key(&info.src_id, secondary), secondary_load + 1);
                if let Some(current) = self.side_info_by_idx.get_mut(&edge_index) {
                    current.src_side = secondary;
                }
            }
        }

        // ----- Step 6.2b: bimodal in/out de-collision for diamond nodes -----
        let in_sides_by_node = self.in_sides_by_node();
        let edge_indices: Vec<usize> = self.side_info_by_idx.keys().copied().collect();
        for edge_index in edge_indices {
            let info = self.side_info_by_idx[&edge_index].clone();
            let src_node = self.node(&info.src_id);
            let shape = node_shape(src_node);
            if !matches!(shape, Some("question") | Some("diamond")) {
                continue;
            }
            let Some(in_sides) = in_sides_by_node.get(&info.src_id) else {
                continue;
            };
            if !in_sides.contains(&info.src_side) {
                continue;
            }
            let secondary = Self::secondary_side(&info);
            if in_sides.contains(&secondary)
                || (*side_load
                    .get(&Self::load_key(&info.src_id, secondary))
                    .unwrap_or(&0))
                    > 0
            {
                continue;
            }
            let primary_load = *side_load
                .get(&Self::load_key(&info.src_id, info.src_side))
                .unwrap_or(&0);
            side_load.insert(
                Self::load_key(&info.src_id, info.src_side),
                (primary_load - 1).max(0),
            );
            side_load.insert(Self::load_key(&info.src_id, secondary), 1);
            if let Some(current) = self.side_info_by_idx.get_mut(&edge_index) {
                current.src_side = secondary;
            }
        }

        // ----- Step 6.2c: keep an in-edge off the face an out-edge leaves ---
        let out_sides_by_node = self.out_sides_by_node();
        let edge_indices: Vec<usize> = self.side_info_by_idx.keys().copied().collect();
        for edge_index in edge_indices {
            let info = self.side_info_by_idx[&edge_index].clone();
            let dst_node = self.node(&info.dst_id);
            let shape = node_shape(dst_node);
            if !matches!(shape, Some("hexagon") | Some("notch-pent")) {
                continue;
            }
            let Some(out_sides) = out_sides_by_node.get(&info.dst_id) else {
                continue;
            };
            if !out_sides.contains(&info.dst_side) {
                continue;
            }
            let secondary = if info.dst_side.is_vertical() {
                if info.dx_sign >= 0.0 {
                    OrthogonalSide::Left
                } else {
                    OrthogonalSide::Right
                }
            } else if info.dy_sign >= 0.0 {
                OrthogonalSide::Top
            } else {
                OrthogonalSide::Bottom
            };
            if out_sides.contains(&secondary)
                || (*side_load
                    .get(&Self::load_key(&info.dst_id, secondary))
                    .unwrap_or(&0))
                    > 0
            {
                continue;
            }
            let primary_load = *side_load
                .get(&Self::load_key(&info.dst_id, info.dst_side))
                .unwrap_or(&0);
            side_load.insert(
                Self::load_key(&info.dst_id, info.dst_side),
                (primary_load - 1).max(0),
            );
            side_load.insert(Self::load_key(&info.dst_id, secondary), 1);
            if let Some(current) = self.side_info_by_idx.get_mut(&edge_index) {
                current.dst_side = secondary;
            }

            // The edge now arrives vertically. If it would still leave the face an in-edge
            // of its source arrives on, leave from the face that looks at the target instead.
            let facing = if info.dy_sign >= 0.0 {
                OrthogonalSide::Bottom
            } else {
                OrthogonalSide::Top
            };
            let src_side_set = in_sides_by_node.get(&info.src_id);
            if info.dst_side.is_vertical()
                && src_side_set.is_some_and(|sides| sides.contains(&info.src_side))
                && src_side_set.is_some_and(|sides| !sides.contains(&facing))
                && (*side_load
                    .get(&Self::load_key(&info.src_id, facing))
                    .unwrap_or(&0))
                    == 0
            {
                let src_load = *side_load
                    .get(&Self::load_key(&info.src_id, info.src_side))
                    .unwrap_or(&0);
                side_load.insert(
                    Self::load_key(&info.src_id, info.src_side),
                    (src_load - 1).max(0),
                );
                side_load.insert(Self::load_key(&info.src_id, facing), 1);
                if let Some(current) = self.side_info_by_idx.get_mut(&edge_index) {
                    current.src_side = facing;
                }
            }
        }
    }

    fn in_sides_by_node(&self) -> HashMap<String, HashSet<OrthogonalSide>> {
        let mut sides: HashMap<String, HashSet<OrthogonalSide>> = HashMap::new();
        for info in self.side_info_by_idx.values() {
            sides
                .entry(info.dst_id.clone())
                .or_default()
                .insert(info.dst_side);
        }
        sides
    }

    fn out_sides_by_node(&self) -> HashMap<String, HashSet<OrthogonalSide>> {
        let mut sides: HashMap<String, HashSet<OrthogonalSide>> = HashMap::new();
        for info in self.side_info_by_idx.values() {
            sides
                .entry(info.src_id.clone())
                .or_default()
                .insert(info.src_side);
        }
        sides
    }

    /// Step 6.3: port-group build, using the possibly-reassigned sides.
    fn build_port_groups(&mut self) {
        let infos: Vec<EdgeSideInfo> = self.side_info_by_idx.values().cloned().collect();
        for info in infos {
            let Some(src) = self.node(&info.src_id) else {
                continue;
            };
            let Some(dst) = self.node(&info.dst_id) else {
                continue;
            };
            let (src_x, src_y) = (src.x.unwrap_or(0.0), src.y.unwrap_or(0.0));
            let (dst_x, dst_y) = (dst.x.unwrap_or(0.0), dst.y.unwrap_or(0.0));

            let src_key = format!("{}:{}:src", info.src_id, info.src_side.as_str());
            let dst_coord = if info.src_side.is_vertical() {
                dst_x
            } else {
                dst_y
            };
            self.port_groups
                .entry(src_key)
                .or_default()
                .push(PortGroupEntry {
                    edge_index: info.edge_index,
                    opposite_coord: dst_coord,
                });

            let dst_key = format!("{}:{}:dst", info.dst_id, info.dst_side.as_str());
            let src_coord = if info.dst_side.is_vertical() {
                src_x
            } else {
                src_y
            };
            self.port_groups
                .entry(dst_key)
                .or_default()
                .push(PortGroupEntry {
                    edge_index: info.edge_index,
                    opposite_coord: src_coord,
                });
        }

        let mut groups: Vec<(String, Vec<PortGroupEntry>)> = Vec::new();
        for (key, mut group) in self.port_groups.clone() {
            group.sort_by(|a, b| {
                a.opposite_coord
                    .partial_cmp(&b.opposite_coord)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            groups.push((key, group));
        }

        for (key, group) in groups {
            if group.len() < 2 {
                continue;
            }
            let parts: Vec<&str> = key.split(':').collect();
            let node_id = parts[..parts.len() - 2].join(":");
            let side = parts[parts.len() - 2];
            let role = parts[parts.len() - 1];
            let Some(node) = self.node(&node_id) else {
                continue;
            };

            let is_vertical_side = side == "left" || side == "right";
            let (drawn_width, drawn_height) = drawn_size(node, 10.0);
            let side_length = if is_vertical_side {
                drawn_height
            } else {
                drawn_width
            };
            let shape = node_shape(self.node(&node_id));
            let is_diamond = matches!(shape, Some("question") | Some("diamond"));
            let effective_length = if is_diamond {
                side_length * 0.3
            } else {
                side_length
            };

            let count = group.len();
            let spacing = MAX_PORT_SPACING
                .min(effective_length / (count as f64 - 1.0))
                .min((MIN_PORT_SPACING).max(effective_length / (count as f64 + 1.0)));
            let total_span = spacing * (count as f64 - 1.0);
            let start_offset = -total_span / 2.0;

            for (position, element) in group.iter().enumerate() {
                let offset = start_offset + position as f64 * spacing;
                self.port_offsets
                    .insert(format!("{}:{role}", element.edge_index), offset);
            }
        }
    }

    fn edge_has_label_node(&self, edge_index: usize) -> bool {
        self.edges
            .get(edge_index)
            .is_some_and(|edge| edge.label_node_id.is_some())
    }

    fn face_has_label_node(&self, node_id: Option<&str>, side: OrthogonalSide) -> bool {
        let Some(node_id) = node_id else {
            return false;
        };
        let src_key = format!("{node_id}:{}:src", side.as_str());
        if self.port_groups.get(&src_key).is_some_and(|group| {
            group
                .iter()
                .any(|entry| self.edge_has_label_node(entry.edge_index))
        }) {
            return true;
        }
        let dst_key = format!("{node_id}:{}:dst", side.as_str());
        self.port_groups.get(&dst_key).is_some_and(|group| {
            group
                .iter()
                .any(|entry| self.edge_has_label_node(entry.edge_index))
        })
    }

    fn apply_port_offset(base_port: Point, side: OrthogonalSide, offset: f64) -> Point {
        if side.is_vertical() {
            Point::new(base_port.x + offset, base_port.y)
        } else {
            Point::new(base_port.x, base_port.y + offset)
        }
    }

    fn ports_for_edge(&self, edge_index: usize, src: &Node, dst: &Node) -> (Point, Point) {
        let side_info = self.side_info_by_idx.get(&edge_index);
        let src_target = Point::new(dst.x.unwrap_or(0.0), dst.y.unwrap_or(0.0));
        let dst_target = Point::new(src.x.unwrap_or(0.0), src.y.unwrap_or(0.0));
        let src_side = side_info
            .map(|info| info.src_side)
            .unwrap_or_else(|| Self::determine_side(src, src_target));
        let dst_side = side_info
            .map(|info| info.dst_side)
            .unwrap_or_else(|| Self::determine_side(dst, dst_target));
        let mut src_port = match side_info {
            Some(info) => Self::port_for_side(src, info.src_side),
            None => Self::get_orthogonal_port(src, src_target, true),
        };
        let mut dst_port = match side_info {
            Some(info) => Self::port_for_side(dst, info.dst_side),
            None => Self::get_orthogonal_port(dst, dst_target, false),
        };

        if let Some(offset) = self.port_offsets.get(&format!("{edge_index}:src")) {
            src_port = Self::apply_port_offset(src_port, src_side, *offset);
        }
        if let Some(offset) = self.port_offsets.get(&format!("{edge_index}:dst")) {
            dst_port = Self::apply_port_offset(dst_port, dst_side, *offset);
        }
        (src_port, dst_port)
    }

    // -----------------------------------------------------------------------
    // Phase 1: initial routing
    // -----------------------------------------------------------------------

    fn run(&mut self) {
        let routing_order = self.routing_order();
        for edge_index in routing_order {
            self.route_edge(edge_index);
        }

        self.reduce_crossings();
        self.rebuild_geometry();
    }

    fn route_edge(&mut self, edge_index: usize) {
        self.edge_segment_indices[edge_index].clear();
        let Some(start) = self.edges[edge_index].start.clone() else {
            return;
        };
        let Some(end) = self.edges[edge_index].end.clone() else {
            return;
        };
        if self.edges[edge_index]
            .points
            .as_ref()
            .is_some_and(|points| !points.is_empty())
        {
            return;
        }
        if start == end {
            return;
        }
        let (Some(src), Some(dst)) = (self.node(&start), self.node(&end)) else {
            return;
        };
        let (src_x, src_y) = (src.x.unwrap_or(0.0), src.y.unwrap_or(0.0));
        let (dst_x, dst_y) = (dst.x.unwrap_or(0.0), dst.y.unwrap_or(0.0));
        let src_port_side = self
            .side_info_by_idx
            .get(&edge_index)
            .map(|info| info.src_side)
            .unwrap_or_else(|| Self::determine_side(src, Point::new(dst_x, dst_y)));
        let dst_port_side = self
            .side_info_by_idx
            .get(&edge_index)
            .map(|info| info.dst_side)
            .unwrap_or_else(|| Self::determine_side(dst, Point::new(src_x, src_y)));

        let (src_port, dst_port) = self.ports_for_edge(edge_index, src, dst);
        let mut src_anchor = src_port;
        let mut dst_anchor = dst_port;

        let src_port_is_vertical = src_port_side.is_vertical();
        let dst_port_is_vertical = dst_port_side.is_vertical();

        if src_port_is_vertical {
            let is_bottom = src_port.y > src_y;
            src_anchor.y = if is_bottom {
                src_port.y + ANCHOR_OFFSET
            } else {
                src_port.y - ANCHOR_OFFSET
            };
        } else {
            let is_right = src_port.x > src_x;
            src_anchor.x = if is_right {
                src_port.x + ANCHOR_OFFSET
            } else {
                src_port.x - ANCHOR_OFFSET
            };
        }

        if dst_port_is_vertical {
            let is_bottom = dst_port.y > dst_y;
            dst_anchor.y = if is_bottom {
                dst_port.y + ANCHOR_OFFSET
            } else {
                dst_port.y - ANCHOR_OFFSET
            };
        } else {
            let is_right = dst_port.x > dst_x;
            dst_anchor.x = if is_right {
                dst_port.x + ANCHOR_OFFSET
            } else {
                dst_port.x - ANCHOR_OFFSET
            };
        }

        let obstacles = self.obstacles.clone();
        let endpoint_ids = [start.clone(), end.clone()];

        let is_point_in_obstacle = |point: Point, exclude: &[String]| -> Option<ObstacleRect> {
            obstacles
                .iter()
                .find(|obstacle| {
                    !exclude.contains(&obstacle.node_id)
                        && point.x > obstacle.min_x
                        && point.x < obstacle.max_x
                        && point.y > obstacle.min_y
                        && point.y < obstacle.max_y
                })
                .cloned()
        };

        let mut src_handle_waypoints: Vec<Point> = Vec::new();
        if let Some(obstacle) = is_point_in_obstacle(src_anchor, &endpoint_ids) {
            if src_port_is_vertical {
                let leaves_positive_side = src_port.y > src_y;
                let go_right = dst_x >= src_port.x;
                let detour = Detour {
                    x: if go_right {
                        obstacle.max_x + HORIZONTAL_PIPE_MARGIN
                    } else {
                        obstacle.min_x - HORIZONTAL_PIPE_MARGIN
                    },
                    y: if leaves_positive_side {
                        obstacle.max_y + VERTICAL_PIPE_MARGIN
                    } else {
                        obstacle.min_y - VERTICAL_PIPE_MARGIN
                    },
                    leaves_positive_side,
                };
                src_anchor.x = detour.x;
                src_anchor.y = detour.y;
                let gap_y = if detour.leaves_positive_side {
                    (obstacle.min_y - 2.0).min(src_port.y + ANCHOR_OFFSET)
                } else {
                    (obstacle.max_y + 2.0).max(src_port.y - ANCHOR_OFFSET)
                };
                src_handle_waypoints = vec![
                    Point::new(src_port.x, gap_y),
                    Point::new(detour.x, gap_y),
                    Point::new(detour.x, detour.y),
                ];
            } else {
                let leaves_positive_side = src_port.x > src_x;
                let go_down = dst_y >= src_port.y;
                let detour = Detour {
                    x: if leaves_positive_side {
                        obstacle.max_x + HORIZONTAL_PIPE_MARGIN
                    } else {
                        obstacle.min_x - HORIZONTAL_PIPE_MARGIN
                    },
                    y: if go_down {
                        obstacle.max_y + VERTICAL_PIPE_MARGIN
                    } else {
                        obstacle.min_y - VERTICAL_PIPE_MARGIN
                    },
                    leaves_positive_side,
                };
                let gap_x = if detour.leaves_positive_side {
                    (obstacle.min_x - 2.0).min(src_port.x + ANCHOR_OFFSET)
                } else {
                    (obstacle.max_x + 2.0).max(src_port.x - ANCHOR_OFFSET)
                };
                src_anchor.x = detour.x;
                src_anchor.y = detour.y;
                src_handle_waypoints = vec![
                    Point::new(gap_x, src_port.y),
                    Point::new(gap_x, detour.y),
                    Point::new(detour.x, detour.y),
                ];
            }
        }

        let mut dst_handle_waypoints: Vec<Point> = Vec::new();
        if let Some(obstacle) = is_point_in_obstacle(dst_anchor, &endpoint_ids) {
            if dst_port_is_vertical {
                let leaves_positive_side = dst_port.y > dst_y;
                let go_right = src_x >= dst_port.x;
                let detour = Detour {
                    x: if go_right {
                        obstacle.max_x + HORIZONTAL_PIPE_MARGIN
                    } else {
                        obstacle.min_x - HORIZONTAL_PIPE_MARGIN
                    },
                    y: if leaves_positive_side {
                        obstacle.max_y + VERTICAL_PIPE_MARGIN
                    } else {
                        obstacle.min_y - VERTICAL_PIPE_MARGIN
                    },
                    leaves_positive_side,
                };
                dst_anchor.x = detour.x;
                dst_anchor.y = detour.y;
                dst_handle_waypoints = vec![
                    Point::new(detour.x, detour.y),
                    Point::new(dst_port.x, detour.y),
                ];
            } else {
                let leaves_positive_side = dst_port.x > dst_x;
                let go_down = src_y >= dst_port.y;
                let detour = Detour {
                    x: if leaves_positive_side {
                        obstacle.max_x + HORIZONTAL_PIPE_MARGIN
                    } else {
                        obstacle.min_x - HORIZONTAL_PIPE_MARGIN
                    },
                    y: if go_down {
                        obstacle.max_y + VERTICAL_PIPE_MARGIN
                    } else {
                        obstacle.min_y - VERTICAL_PIPE_MARGIN
                    },
                    leaves_positive_side,
                };
                dst_anchor.x = detour.x;
                dst_anchor.y = detour.y;
                dst_handle_waypoints = vec![
                    Point::new(detour.x, detour.y),
                    Point::new(detour.x, dst_port.y),
                ];
            }
        }

        // ----- Centered straight-line fast path -----
        if src_handle_waypoints.is_empty() && dst_handle_waypoints.is_empty() {
            let horizontal_margin = HORIZONTAL_PIPE_MARGIN;
            let anchors_same_x = (src_anchor.x - dst_anchor.x).abs() < horizontal_margin;
            let anchors_same_y = (src_anchor.y - dst_anchor.y).abs() < horizontal_margin;
            let has_port_offset = self.port_offsets.contains_key(&format!("{edge_index}:src"))
                || self.port_offsets.contains_key(&format!("{edge_index}:dst"));
            let src_face_total = self.face_total(&start, src_port_side);
            let dst_face_total = self.face_total(&end, dst_port_side);
            let face_contested = src_face_total > 1 || dst_face_total > 1;
            let src_incident_total = self.incident_edge_totals.get(&start).copied().unwrap_or(0);
            let dst_incident_total = self.incident_edge_totals.get(&end).copied().unwrap_or(0);
            let contested_face_has_label = (src_face_total > 1
                && self.face_has_label_node(Some(&start), src_port_side))
                || (dst_face_total > 1 && self.face_has_label_node(Some(&end), dst_port_side));
            let src_contest_allows = src_face_total <= 1 || src_incident_total <= 2;
            let dst_contest_allows = dst_face_total <= 1 || dst_incident_total <= 2;
            let can_preserve_simple_contested_straight = face_contested
                && !contested_face_has_label
                && src_contest_allows
                && dst_contest_allows;
            if (anchors_same_x || anchors_same_y)
                && !has_port_offset
                && (!face_contested || can_preserve_simple_contested_straight)
            {
                let direct_blocked =
                    self.is_segment_blocked(src_port, dst_port, Some(&start), Some(&end));
                if !direct_blocked {
                    self.edges[edge_index].points =
                        Some(vec![src_port, src_anchor, dst_anchor, dst_port]);
                    self.straight_intra_lane_edges.insert(edge_index);
                    let fast_path_orientation = if anchors_same_y {
                        Orientation::Horizontal
                    } else {
                        Orientation::Vertical
                    };
                    let fast_path_coord = if anchors_same_y {
                        src_port.y
                    } else {
                        src_port.x
                    };
                    let fast_path_from = if anchors_same_y {
                        src_port.x.min(dst_port.x)
                    } else {
                        src_port.y.min(dst_port.y)
                    };
                    let fast_path_to = if anchors_same_y {
                        src_port.x.max(dst_port.x)
                    } else {
                        src_port.y.max(dst_port.y)
                    };
                    self.all_routed_segments.push(RoutedSegment {
                        edge_index,
                        segment_index: 0,
                        orientation: fast_path_orientation,
                        pipe: None,
                        pipe_coord: fast_path_coord,
                        track_index: 0,
                        from: fast_path_from,
                        to: fast_path_to,
                    });
                    return;
                }
            }
        }

        // Snap anchors to the nearest pipe (created lazily).
        let src_pipe = self.get_or_add_pipe(
            Orientation::Vertical,
            src_anchor.x,
            src_anchor.y,
            src_anchor.y,
        );
        src_anchor.x = self.pipes[src_pipe].coord;
        let dst_pipe = self.get_or_add_pipe(
            Orientation::Vertical,
            dst_anchor.x,
            dst_anchor.y,
            dst_anchor.y,
        );
        dst_anchor.x = self.pipes[dst_pipe].coord;

        // 4. Visibility graph and pathfinding.
        let mut bb_min_x = src_anchor.x.min(dst_anchor.x) - 50.0;
        let mut bb_max_x = src_anchor.x.max(dst_anchor.x) + 50.0;
        let mut bb_min_y = src_anchor.y.min(dst_anchor.y) - 50.0;
        let mut bb_max_y = src_anchor.y.max(dst_anchor.y) + 50.0;

        for obstacle in &obstacles {
            let path_min_x = src_anchor.x.min(dst_anchor.x);
            let path_max_x = src_anchor.x.max(dst_anchor.x);
            let path_min_y = src_anchor.y.min(dst_anchor.y);
            let path_max_y = src_anchor.y.max(dst_anchor.y);

            let blocks_path = obstacle.min_x < path_max_x
                && obstacle.max_x > path_min_x
                && obstacle.min_y < path_max_y
                && obstacle.max_y > path_min_y;

            if blocks_path {
                bb_min_x = bb_min_x.min(obstacle.min_x - ROUTING_MARGIN);
                bb_max_x = bb_max_x.max(obstacle.max_x + ROUTING_MARGIN);
                bb_min_y = bb_min_y.min(obstacle.min_y - ROUTING_MARGIN);
                bb_max_y = bb_max_y.max(obstacle.max_y + ROUTING_MARGIN);
            }
        }

        for obstacle in &obstacles {
            if obstacle.max_x < bb_min_x
                || obstacle.min_x > bb_max_x
                || obstacle.max_y < bb_min_y
                || obstacle.min_y > bb_max_y
            {
                continue;
            }
            self.get_or_add_pipe(
                Orientation::Horizontal,
                obstacle.min_y - HORIZONTAL_PIPE_MARGIN,
                bb_min_x,
                bb_max_x,
            );
            self.get_or_add_pipe(
                Orientation::Horizontal,
                obstacle.max_y + HORIZONTAL_PIPE_MARGIN,
                bb_min_x,
                bb_max_x,
            );
            self.get_or_add_pipe(
                Orientation::Vertical,
                obstacle.min_x - VERTICAL_PIPE_MARGIN,
                bb_min_y,
                bb_max_y,
            );
            self.get_or_add_pipe(
                Orientation::Vertical,
                obstacle.max_x + VERTICAL_PIPE_MARGIN,
                bb_min_y,
                bb_max_y,
            );
        }

        self.get_or_add_pipe(Orientation::Horizontal, src_anchor.y, bb_min_x, bb_max_x);
        self.get_or_add_pipe(Orientation::Horizontal, dst_anchor.y, bb_min_x, bb_max_x);

        let mut h_pipes: Vec<usize> = (0..self.pipes.len())
            .filter(|&pipe| {
                self.pipes[pipe].orientation == Orientation::Horizontal
                    && self.pipes[pipe].coord >= bb_min_y
                    && self.pipes[pipe].coord <= bb_max_y
            })
            .collect();
        let mut v_pipes: Vec<usize> = (0..self.pipes.len())
            .filter(|&pipe| {
                self.pipes[pipe].orientation == Orientation::Vertical
                    && self.pipes[pipe].coord >= bb_min_x
                    && self.pipes[pipe].coord <= bb_max_x
            })
            .collect();
        h_pipes.sort_by(|&a, &b| {
            self.pipes[a]
                .coord
                .partial_cmp(&self.pipes[b].coord)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        v_pipes.sort_by(|&a, &b| {
            self.pipes[a]
                .coord
                .partial_cmp(&self.pipes[b].coord)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let get_key = |x: f64, y: f64| format!("{},{}", to_fixed(x, 1), to_fixed(y, 1));
        let start_key = get_key(src_anchor.x, src_anchor.y);
        let end_key = get_key(dst_anchor.x, dst_anchor.y);

        let mut g_score: HashMap<String, f64> = HashMap::new();
        let mut came_from: HashMap<String, Point> = HashMap::new();
        let mut arrival_dir: HashMap<String, char> = HashMap::new();
        let mut open_set: HashSet<String> = HashSet::new();
        let mut open_list: Vec<OpenEntry> = Vec::new();

        g_score.insert(start_key.clone(), 0.0);
        arrival_dir.insert(start_key.clone(), 'n');
        open_list.push(OpenEntry {
            key: start_key.clone(),
            f: (dst_anchor.x - src_anchor.x).hypot(dst_anchor.y - src_anchor.y),
            pt: src_anchor,
        });
        open_set.insert(start_key.clone());

        let mut found_path: Vec<Point> = Vec::new();

        let check_segment_blocked =
            |p1: Point, p2: Point| self.is_segment_blocked(p1, p2, Some(&start), Some(&end));

        let corner_hv = Point::new(dst_anchor.x, src_anchor.y);
        let path_hv_blocked = check_segment_blocked(src_anchor, corner_hv)
            || check_segment_blocked(corner_hv, dst_anchor);
        let corner_vh = Point::new(src_anchor.x, dst_anchor.y);
        let path_vh_blocked = check_segment_blocked(src_anchor, corner_vh)
            || check_segment_blocked(corner_vh, dst_anchor);

        if !path_hv_blocked {
            if (src_anchor.y - dst_anchor.y).abs() < EPS
                || (src_anchor.x - dst_anchor.x).abs() < EPS
            {
                found_path = vec![src_anchor, dst_anchor];
            } else {
                found_path = vec![src_anchor, corner_hv, dst_anchor];
            }
        } else if !path_vh_blocked {
            if (src_anchor.x - dst_anchor.x).abs() < EPS {
                found_path = vec![src_anchor, dst_anchor];
            } else {
                found_path = vec![src_anchor, corner_vh, dst_anchor];
            }
        }

        if found_path.is_empty() {
            while !open_list.is_empty() {
                open_list
                    .sort_by(|a, b| a.f.partial_cmp(&b.f).unwrap_or(std::cmp::Ordering::Equal));
                let current = open_list.remove(0);
                open_set.remove(&current.key);

                if current.key == end_key {
                    let mut reversed: Vec<Point> = vec![dst_anchor];
                    let mut current_key = end_key.clone();
                    while let Some(previous) = came_from.get(&current_key) {
                        reversed.push(*previous);
                        current_key = get_key(previous.x, previous.y);
                    }
                    reversed.reverse();
                    found_path = reversed;
                    break;
                }

                let cx = current.pt.x;
                let cy = current.pt.y;

                let v_index = v_pipes
                    .iter()
                    .position(|&pipe| (self.pipes[pipe].coord - cx).abs() < 1.0);
                let h_index = h_pipes
                    .iter()
                    .position(|&pipe| (self.pipes[pipe].coord - cy).abs() < 1.0);

                let mut neighbors: Vec<Point> = Vec::new();
                if let Some(index) = v_index {
                    if index > 0 {
                        neighbors.push(Point::new(self.pipes[v_pipes[index - 1]].coord, cy));
                    }
                    if index + 1 < v_pipes.len() {
                        neighbors.push(Point::new(self.pipes[v_pipes[index + 1]].coord, cy));
                    }
                }
                if let Some(index) = h_index {
                    if index > 0 {
                        neighbors.push(Point::new(cx, self.pipes[h_pipes[index - 1]].coord));
                    }
                    if index + 1 < h_pipes.len() {
                        neighbors.push(Point::new(cx, self.pipes[h_pipes[index + 1]].coord));
                    }
                }

                for neighbor in neighbors {
                    let min_x = cx.min(neighbor.x);
                    let max_x = cx.max(neighbor.x);
                    let min_y = cy.min(neighbor.y);
                    let max_y = cy.max(neighbor.y);

                    let blocked = obstacles.iter().any(|obstacle| {
                        if obstacle.node_id == start || obstacle.node_id == end {
                            return false;
                        }
                        if min_x != max_x {
                            obstacle.min_y < cy
                                && obstacle.max_y > cy
                                && obstacle.max_x > min_x
                                && obstacle.min_x < max_x
                        } else {
                            obstacle.min_x < cx
                                && obstacle.max_x > cx
                                && obstacle.max_y > min_y
                                && obstacle.min_y < max_y
                        }
                    });
                    if blocked {
                        continue;
                    }

                    let neighbor_key = get_key(neighbor.x, neighbor.y);
                    let distance = (neighbor.x - cx).abs() + (neighbor.y - cy).abs();
                    let penalty = self.crossing_penalty(edge_index, current.pt, neighbor);

                    let dest_dx = dst_anchor.x - src_anchor.x;
                    let dest_dy = dst_anchor.y - src_anchor.y;
                    let move_dx = neighbor.x - cx;
                    let move_dy = neighbor.y - cy;

                    let mut direction_penalty = 0.0;
                    if (dest_dy > 10.0 && move_dy < -5.0) || (dest_dy < -10.0 && move_dy > 5.0) {
                        direction_penalty = move_dy.abs() * 100.0;
                    }
                    if (dest_dx > 10.0 && move_dx < -5.0) || (dest_dx < -10.0 && move_dx > 5.0) {
                        direction_penalty += move_dx.abs() * 50.0;
                    }

                    let mut bend_penalty = 0.0;
                    let current_dir = arrival_dir.get(&current.key).copied().unwrap_or('n');
                    let move_dir = if move_dx.abs() > EPS { 'h' } else { 'v' };
                    if current_dir != 'n' && current_dir != move_dir {
                        bend_penalty = 50.0;
                    }

                    let step_cost = distance + penalty + direction_penalty + bend_penalty;
                    let tentative_g =
                        g_score.get(&current.key).copied().unwrap_or(f64::INFINITY) + step_cost;
                    let heuristic =
                        (dst_anchor.x - neighbor.x).abs() + (dst_anchor.y - neighbor.y).abs();

                    if tentative_g < g_score.get(&neighbor_key).copied().unwrap_or(f64::INFINITY) {
                        came_from.insert(neighbor_key.clone(), current.pt);
                        g_score.insert(neighbor_key.clone(), tentative_g);
                        arrival_dir.insert(neighbor_key.clone(), move_dir);
                        if !open_set.contains(&neighbor_key) {
                            open_list.push(OpenEntry {
                                key: neighbor_key.clone(),
                                f: tentative_g + heuristic,
                                pt: neighbor,
                            });
                            open_set.insert(neighbor_key);
                        } else if let Some(entry) =
                            open_list.iter_mut().find(|entry| entry.key == neighbor_key)
                        {
                            entry.f = tentative_g + heuristic;
                        }
                    }
                }
            }
        }

        if found_path.is_empty() {
            found_path = vec![
                src_anchor,
                Point::new(src_anchor.x, dst_anchor.y),
                dst_anchor,
            ];
        }

        // Path simplification: keep only the extents needed to clear obstacles.
        if found_path.len() > 4 {
            let start_point = found_path[0];
            let end_point = found_path[found_path.len() - 1];

            let mut min_x = start_point.x.min(end_point.x);
            let mut max_x = start_point.x.max(end_point.x);
            let mut min_y = start_point.y.min(end_point.y);
            let mut max_y = start_point.y.max(end_point.y);

            for point in &found_path {
                min_x = min_x.min(point.x);
                max_x = max_x.max(point.x);
                min_y = min_y.min(point.y);
                max_y = max_y.max(point.y);
            }

            let went_right = max_x > start_point.x.max(end_point.x);
            let went_left = min_x < start_point.x.min(end_point.x);

            if self.is_lr {
                let margin = VERTICAL_PIPE_MARGIN;
                if went_right {
                    let path_x = start_point.x.max(end_point.x);
                    let path_min_y = start_point.y.min(end_point.y);
                    let path_max_y = start_point.y.max(end_point.y);
                    let detour_obstacles: Vec<&ObstacleRect> = obstacles
                        .iter()
                        .filter(|obstacle| {
                            obstacle.min_x < path_x
                                && obstacle.max_x > path_x
                                && obstacle.min_y < path_max_y
                                && obstacle.max_y > path_min_y
                        })
                        .collect();
                    if !detour_obstacles.is_empty() {
                        let mut visual_max_x = start_point.x.max(end_point.x);
                        for obstacle in detour_obstacles {
                            let obstacle_center_x = (obstacle.min_x + obstacle.max_x) / 2.0;
                            let visual_right =
                                obstacle_center_x + obstacle.visual_x_half_extent + margin;
                            visual_max_x = visual_max_x.max(visual_right);
                        }
                        max_x = visual_max_x;
                    }
                }
                if went_left {
                    let detour_obstacles: Vec<&ObstacleRect> = obstacles
                        .iter()
                        .filter(|obstacle| {
                            obstacle.min_x < start_point.x.min(end_point.x) + margin
                                && obstacle.min_y < start_point.y.max(end_point.y)
                                && obstacle.max_y > start_point.y.min(end_point.y)
                        })
                        .collect();
                    if !detour_obstacles.is_empty() {
                        let mut visual_min_x = start_point.x.min(end_point.x);
                        for obstacle in detour_obstacles {
                            let obstacle_center_x = (obstacle.min_x + obstacle.max_x) / 2.0;
                            let visual_left =
                                obstacle_center_x - obstacle.visual_x_half_extent - margin;
                            visual_min_x = visual_min_x.min(visual_left);
                        }
                        min_x = visual_min_x;
                    }
                }
            }

            let find_best_return_y = |detour_x: f64| -> f64 {
                let going_down = end_point.y > start_point.y;
                let relevant: Vec<&ObstacleRect> = obstacles
                    .iter()
                    .filter(|obstacle| {
                        let in_x = start_point.x.min(end_point.x) < obstacle.max_x
                            && start_point.x.max(end_point.x) > obstacle.min_x;
                        let in_y = start_point.y.min(end_point.y) < obstacle.max_y
                            && start_point.y.max(end_point.y) > obstacle.min_y;
                        in_x && in_y
                    })
                    .collect();

                let mut filtered = relevant;
                if self.is_lr && !filtered.is_empty() {
                    let at_detour: Vec<&ObstacleRect> = filtered
                        .iter()
                        .copied()
                        .filter(|obstacle| obstacle.min_x < detour_x && obstacle.max_x > detour_x)
                        .collect();
                    if !at_detour.is_empty() {
                        filtered = at_detour;
                    }
                }
                if filtered.is_empty() {
                    return end_point.y;
                }

                let margin = HORIZONTAL_PIPE_MARGIN;
                if going_down {
                    let lowest = filtered
                        .iter()
                        .map(|obstacle| obstacle.max_y)
                        .fold(f64::NEG_INFINITY, f64::max);
                    let best_y = lowest + margin;
                    if best_y < end_point.y - EPS {
                        return best_y;
                    }
                } else {
                    let highest = filtered
                        .iter()
                        .map(|obstacle| obstacle.min_y)
                        .fold(f64::INFINITY, f64::min);
                    let best_y = highest - margin;
                    if best_y > end_point.y + EPS {
                        return best_y;
                    }
                }
                end_point.y
            };

            let try_simplify_with_detour_x = |detour_x: f64| -> Option<Vec<Point>> {
                let best_y = find_best_return_y(detour_x);
                let corner1 = Point::new(detour_x, start_point.y);
                let corner2 = Point::new(detour_x, best_y);
                let corner3 = Point::new(end_point.x, best_y);
                let seg1_blocked = check_segment_blocked(start_point, corner1);
                let seg2_blocked = check_segment_blocked(corner1, corner2);
                let seg3_blocked = check_segment_blocked(corner2, corner3);
                let seg4_blocked = (best_y - end_point.y).abs() >= EPS
                    && check_segment_blocked(corner3, end_point);
                if !seg1_blocked && !seg2_blocked && !seg3_blocked && !seg4_blocked {
                    if (best_y - end_point.y).abs() < EPS {
                        return Some(vec![start_point, corner1, corner2, end_point]);
                    }
                    return Some(vec![start_point, corner1, corner2, corner3, end_point]);
                }
                None
            };

            let simplified = if went_right && !went_left {
                try_simplify_with_detour_x(max_x)
            } else if went_left && !went_right {
                try_simplify_with_detour_x(min_x)
            } else {
                None
            };
            if let Some(simplified) = simplified {
                found_path = simplified;
            }
        }

        // 5. Collapse collinear runs and generate segments.
        let mut full_points = vec![src_port];
        full_points.extend(src_handle_waypoints.iter().copied());
        full_points.extend(found_path.iter().copied());
        full_points.extend(dst_handle_waypoints.iter().rev().copied());
        full_points.push(dst_port);

        // Remove hooks / overshoots at the end.
        if full_points.len() >= 3 {
            let last = full_points.len();
            let c = full_points[last - 1];
            let b = full_points[last - 2];
            let a = full_points[last - 3];

            let is_horizontal = (a.y - b.y).abs() < EPS && (b.y - c.y).abs() < EPS;
            let is_vertical = (a.x - b.x).abs() < EPS && (b.x - c.x).abs() < EPS;

            if is_horizontal {
                let sign_ab = js_sign(b.x - a.x);
                let sign_ac = js_sign(c.x - a.x);
                if sign_ab != 0.0 && sign_ab == sign_ac && (b.x - a.x).abs() > (c.x - a.x).abs() {
                    full_points.remove(last - 2);
                }
            } else if is_vertical {
                let sign_ab = js_sign(b.y - a.y);
                let sign_ac = js_sign(c.y - a.y);
                if sign_ab != 0.0 && sign_ab == sign_ac && (b.y - a.y).abs() > (c.y - a.y).abs() {
                    full_points.remove(last - 2);
                }
            }
        }

        let mut simplified: Vec<Point> = vec![full_points[0]];
        for k in 1..full_points.len().saturating_sub(1) {
            if k == 1 {
                simplified.push(full_points[k]);
                continue;
            }
            let previous = simplified[simplified.len() - 1];
            let current = full_points[k];
            let next = full_points[k + 1];

            if (previous.y - current.y).abs() < EPS && (current.y - next.y).abs() < EPS {
                let dir1 = current.x > previous.x;
                let dir2 = next.x > current.x;
                if dir1 != dir2 {
                    simplified.push(current);
                }
                continue;
            }
            if (previous.x - current.x).abs() < EPS && (current.x - next.x).abs() < EPS {
                let dir1 = current.y > previous.y;
                let dir2 = next.y > current.y;
                if dir1 != dir2 {
                    simplified.push(current);
                }
                continue;
            }
            simplified.push(current);
        }
        simplified.push(full_points[full_points.len() - 1]);

        for k in 0..simplified.len() - 1 {
            let p1 = simplified[k];
            let p2 = simplified[k + 1];
            let orientation = if (p1.x - p2.x).abs() < EPS {
                Orientation::Vertical
            } else {
                Orientation::Horizontal
            };
            let coord = if orientation == Orientation::Vertical {
                p1.x
            } else {
                p1.y
            };
            let from = if orientation == Orientation::Vertical {
                p1.y.min(p2.y)
            } else {
                p1.x.min(p2.x)
            };
            let to = if orientation == Orientation::Vertical {
                p1.y.max(p2.y)
            } else {
                p1.x.max(p2.x)
            };

            let pipe = self.get_or_add_pipe(orientation, coord, from, to);
            self.all_routed_segments.push(RoutedSegment {
                edge_index,
                segment_index: k,
                orientation,
                pipe: Some(pipe),
                pipe_coord: self.pipes[pipe].coord,
                track_index: 0,
                from,
                to,
            });
            let segment_position = self.all_routed_segments.len() - 1;
            self.edge_segment_indices[edge_index].push(segment_position);

            if self.pipes[pipe].tracks.is_empty() {
                let pipe_coord = self.pipes[pipe].coord;
                self.pipes[pipe].tracks.push(Track {
                    index: 0,
                    coord: pipe_coord,
                    segments: Vec::new(),
                });
            }
            self.pipes[pipe].tracks[0].segments.push(SegmentRef {
                edge_index,
                segment_index: k,
                from,
                to,
            });
        }
    }

    fn face_total(&self, node_id: &str, side: OrthogonalSide) -> usize {
        let src_key = format!("{node_id}:{}:src", side.as_str());
        let dst_key = format!("{node_id}:{}:dst", side.as_str());
        self.port_groups.get(&src_key).map(Vec::len).unwrap_or(0)
            + self.port_groups.get(&dst_key).map(Vec::len).unwrap_or(0)
    }

    // -----------------------------------------------------------------------
    // Phase 2: crossing reduction
    // -----------------------------------------------------------------------

    fn reduce_crossings(&mut self) {
        let mut iterations = 0;
        while iterations < MAX_ITERATIONS {
            let mut changed = 0;
            changed += self.fix_source_handle_crossings();
            changed += self.fix_target_handle_crossings();
            changed += self.fix_pipe_crossings();
            if changed == 0 {
                break;
            }
            iterations += 1;
        }
    }

    fn get_dest_info(&self, edge_index: usize) -> DestInfo {
        let indices = &self.edge_segment_indices[edge_index];
        if indices.is_empty() {
            return DestInfo::default();
        }
        let first = &self.all_routed_segments[indices[0]];
        let base = first.pipe_coord;
        let mut dest = base;
        for &index in &indices[1..] {
            let segment = &self.all_routed_segments[index];
            if segment.orientation == Orientation::Horizontal {
                dest = if (segment.from - base).abs() > (segment.to - base).abs() {
                    segment.from
                } else {
                    segment.to
                };
                break;
            }
        }
        DestInfo {
            dest,
            deviation: (dest - base).abs(),
            delta: dest - base,
        }
    }

    fn edge_distance(&self, edge_index: usize) -> f64 {
        let edge = &self.edges[edge_index];
        let (Some(start), Some(end)) = (&edge.start, &edge.end) else {
            return 0.0;
        };
        let (Some(src), Some(dst)) = (self.node(start), self.node(end)) else {
            return 0.0;
        };
        let dx = dst.x.unwrap_or(0.0) - src.x.unwrap_or(0.0);
        let dy = dst.y.unwrap_or(0.0) - src.y.unwrap_or(0.0);
        dx.abs() + dy.abs()
    }

    fn fix_source_handle_crossings(&mut self) -> usize {
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut group_index: HashMap<String, usize> = HashMap::new();
        for (index, edge) in self.edges.iter().enumerate() {
            if self.edge_segment_indices[index].is_empty() {
                continue;
            }
            let Some(start) = &edge.start else {
                continue;
            };
            push_ordered_group(&mut groups, &mut group_index, start, index);
        }

        let mut crossings = 0;
        for group in &mut groups {
            group.sort_by(|&a, &b| {
                let info_a = self.get_dest_info(a);
                let info_b = self.get_dest_info(b);
                if (info_a.deviation - info_b.deviation).abs() > 1.0 {
                    return info_a
                        .deviation
                        .partial_cmp(&info_b.deviation)
                        .unwrap_or(std::cmp::Ordering::Equal);
                }
                if (info_a.dest - info_b.dest).abs() > 1.0 {
                    return info_a
                        .dest
                        .partial_cmp(&info_b.dest)
                        .unwrap_or(std::cmp::Ordering::Equal);
                }

                let distance_a = self.edge_distance(a);
                let distance_b = self.edge_distance(b);
                if (distance_a - distance_b).abs() > 1.0 {
                    return distance_b
                        .partial_cmp(&distance_a)
                        .unwrap_or(std::cmp::Ordering::Equal);
                }

                let length_a = self.edge_segment_indices[a].len();
                let length_b = self.edge_segment_indices[b].len();
                if length_a != length_b {
                    return length_a.cmp(&length_b);
                }

                if length_a == 1 {
                    let index_a = self.edge_segment_indices[a][0];
                    let index_b = self.edge_segment_indices[b][0];
                    let segment_a = &self.all_routed_segments[index_a];
                    let segment_b = &self.all_routed_segments[index_b];
                    let span_a = (segment_a.to - segment_a.from).abs();
                    let span_b = (segment_b.to - segment_b.from).abs();
                    if (span_a - span_b).abs() > 1.0 {
                        return span_a
                            .partial_cmp(&span_b)
                            .unwrap_or(std::cmp::Ordering::Equal);
                    }
                }
                std::cmp::Ordering::Equal
            });

            let handles: Vec<usize> = group
                .iter()
                .map(|&edge_index| self.edge_segment_indices[edge_index][0])
                .collect();
            crossings += self.resolve_handle_conflicts(&handles);
        }
        crossings
    }

    fn fix_target_handle_crossings(&mut self) -> usize {
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut group_index: HashMap<String, usize> = HashMap::new();
        for (index, edge) in self.edges.iter().enumerate() {
            if self.edge_segment_indices[index].is_empty() {
                continue;
            }
            let Some(end) = &edge.end else {
                continue;
            };
            push_ordered_group(&mut groups, &mut group_index, end, index);
        }

        let mut crossings = 0;
        for group in &mut groups {
            group.sort_by(|&a, &b| {
                let distance = |edge_index: usize| -> f64 {
                    let indices = &self.edge_segment_indices[edge_index];
                    if indices.len() < 2 {
                        return 0.0;
                    }
                    let previous = &self.all_routed_segments[indices[indices.len() - 2]];
                    (previous.to - previous.from).abs()
                };
                let score_a = distance(a);
                let score_b = distance(b);
                if (score_a - score_b).abs() > 0.1 {
                    return score_a
                        .partial_cmp(&score_b)
                        .unwrap_or(std::cmp::Ordering::Equal);
                }
                a.cmp(&b)
            });

            let handles: Vec<usize> = group
                .iter()
                .map(|&edge_index| {
                    let indices = &self.edge_segment_indices[edge_index];
                    indices[indices.len() - 1]
                })
                .collect();
            crossings += self.resolve_handle_conflicts(&handles);
        }
        crossings
    }

    fn fix_pipe_crossings(&mut self) -> usize {
        let mut crossings = 0;
        for pipe_index in 0..self.pipes.len() {
            let mut pipe_segments: Vec<usize> = Vec::new();
            for track in &self.pipes[pipe_index].tracks {
                for reference in &track.segments {
                    let found = self.edge_segment_indices[reference.edge_index]
                        .iter()
                        .copied()
                        .find(|&index| {
                            self.all_routed_segments[index].segment_index == reference.segment_index
                        });
                    if let Some(index) = found {
                        pipe_segments.push(index);
                    }
                }
            }

            pipe_segments.sort_by(|&a, &b| {
                let segment_a = &self.all_routed_segments[a];
                let segment_b = &self.all_routed_segments[b];
                segment_a
                    .edge_index
                    .cmp(&segment_b.edge_index)
                    .then(segment_a.segment_index.cmp(&segment_b.segment_index))
            });

            for i in 0..pipe_segments.len() {
                for j in (i + 1)..pipe_segments.len() {
                    let s1 = pipe_segments[i];
                    let s2 = pipe_segments[j];
                    if self.segments_conflict(s1, s2) {
                        crossings += 1;
                        self.resolve_track_conflict(s1, s2, MoveMode::Segment);
                    }
                }
            }
        }
        crossings
    }

    fn resolve_handle_conflicts(&mut self, handles: &[usize]) -> usize {
        let mut crossings = 0;
        for i in 0..handles.len() {
            for j in (i + 1)..handles.len() {
                let h1 = handles[i];
                let h2 = handles[j];
                if self.all_routed_segments[h1].pipe != self.all_routed_segments[h2].pipe {
                    continue;
                }
                if self.segments_conflict(h1, h2) {
                    crossings += 1;
                    self.resolve_track_conflict(h1, h2, MoveMode::Chain);
                }
            }
        }
        crossings
    }

    fn segments_conflict(&self, first: usize, second: usize) -> bool {
        let s1 = &self.all_routed_segments[first];
        let s2 = &self.all_routed_segments[second];
        if s1.track_index == s2.track_index {
            return segments_overlap(&segment_ref(s1), &segment_ref(s2));
        }

        let adjacent1 = self.adjacent_segments_along_edge(first);
        let adjacent2 = self.adjacent_segments_along_edge(second);
        adjacent1
            .iter()
            .any(|&a1| adjacent2.iter().any(|&a2| self.have_any_crossing(a1, a2)))
    }

    fn adjacent_segments_along_edge(&self, segment_index: usize) -> Vec<usize> {
        let segment = &self.all_routed_segments[segment_index];
        let indices = &self.edge_segment_indices[segment.edge_index];
        let position = indices
            .iter()
            .position(|&index| index == segment_index)
            .map(|position| position as isize)
            .unwrap_or(-1);
        let mut adjacent = Vec::new();
        if position > 0 {
            adjacent.push(indices[(position - 1) as usize]);
        }
        if position < indices.len() as isize - 1 {
            adjacent.push(indices[(position + 1) as usize]);
        }
        adjacent
    }

    fn have_any_crossing(&self, first: usize, second: usize) -> bool {
        let s1 = &self.all_routed_segments[first];
        let s2 = &self.all_routed_segments[second];
        if s1.orientation == s2.orientation {
            return false;
        }
        let (horizontal, vertical) = if s1.orientation == Orientation::Horizontal {
            (s1, s2)
        } else {
            (s2, s1)
        };
        vertical.pipe_coord > horizontal.from
            && vertical.pipe_coord < horizontal.to
            && horizontal.pipe_coord > vertical.from
            && horizontal.pipe_coord < vertical.to
    }

    fn resolve_track_conflict(&mut self, s1: usize, s2: usize, mode: MoveMode) {
        if self.try_swap_segments_across_tracks(s1, s2) {
            return;
        }
        let pipe = self.all_routed_segments[s1].pipe;
        let available = pipe.and_then(|pipe| self.find_available_track(pipe, s2));
        let target_track = match (available, pipe) {
            (Some(track), _) => track,
            (None, Some(pipe)) => self.create_new_track(pipe),
            (None, None) => return,
        };
        match mode {
            MoveMode::Segment => self.move_segment_to_track(s2, target_track),
            MoveMode::Chain => self.move_segment_chain_to_track(s2, target_track),
        }
    }

    fn try_swap_segments_across_tracks(&mut self, first: usize, second: usize) -> bool {
        let s1 = self.all_routed_segments[first].clone();
        let s2 = self.all_routed_segments[second].clone();
        let (Some(p1), Some(p2)) = (s1.pipe, s2.pipe) else {
            return false;
        };
        let t1 = s1.track_index;
        let t2 = s2.track_index;

        let can_s1_go_t2 = !self.pipes[p2].tracks[t2].segments.iter().any(|reference| {
            (reference.edge_index != s2.edge_index || reference.segment_index != s2.segment_index)
                && segments_overlap(reference, &segment_ref(&s1))
        });
        let can_s2_go_t1 = !self.pipes[p1].tracks[t1].segments.iter().any(|reference| {
            (reference.edge_index != s1.edge_index || reference.segment_index != s1.segment_index)
                && segments_overlap(reference, &segment_ref(&s2))
        });

        if !(can_s1_go_t2 && can_s2_go_t1) {
            return false;
        }

        self.all_routed_segments[first].track_index = t2;
        self.all_routed_segments[second].track_index = t1;

        let mut first_track = self.pipes[p1].tracks[t1].segments.clone();
        first_track.retain(|reference| {
            reference.edge_index != s1.edge_index || reference.segment_index != s1.segment_index
        });
        first_track.push(segment_ref(&s2));
        self.pipes[p1].tracks[t1].segments = first_track;

        let mut second_track = self.pipes[p2].tracks[t2].segments.clone();
        second_track.retain(|reference| {
            reference.edge_index != s2.edge_index || reference.segment_index != s2.segment_index
        });
        second_track.push(segment_ref(&s1));
        self.pipes[p2].tracks[t2].segments = second_track;
        true
    }

    fn find_available_track(&self, pipe_index: usize, segment_index: usize) -> Option<usize> {
        let segment = &self.all_routed_segments[segment_index];
        for track in &self.pipes[pipe_index].tracks {
            let overlap = track.segments.iter().any(|reference| {
                (reference.edge_index != segment.edge_index
                    || reference.segment_index != segment.segment_index)
                    && segments_overlap(reference, &segment_ref(segment))
            });
            if !overlap {
                return Some(track.index);
            }
        }
        None
    }

    fn create_new_track(&mut self, pipe_index: usize) -> usize {
        let index = self.pipes[pipe_index].tracks.len();
        let coord = self.pipes[pipe_index].coord;
        self.pipes[pipe_index].tracks.push(Track {
            index,
            coord,
            segments: Vec::new(),
        });
        index
    }

    fn move_segment_to_track(&mut self, segment_index: usize, track_index: usize) {
        let segment = self.all_routed_segments[segment_index].clone();
        let Some(pipe) = segment.pipe else {
            return;
        };
        self.pipes[pipe].tracks[segment.track_index]
            .segments
            .retain(|reference| {
                reference.edge_index != segment.edge_index
                    || reference.segment_index != segment.segment_index
            });
        self.all_routed_segments[segment_index].track_index = track_index;
        self.pipes[pipe].tracks[track_index]
            .segments
            .push(segment_ref(&segment));
    }

    fn move_segment_chain_to_track(&mut self, segment_index: usize, track_index: usize) {
        let segment = self.all_routed_segments[segment_index].clone();
        let indices = self.edge_segment_indices[segment.edge_index].clone();
        for index in indices {
            if self.all_routed_segments[index].pipe == segment.pipe {
                self.move_segment_to_track(index, track_index);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Phase 3: rebuild geometry
    // -----------------------------------------------------------------------

    fn rebuild_geometry(&mut self) {
        let mut segment_coords: HashMap<(usize, usize), f64> = HashMap::new();

        for pipe_index in 0..self.pipes.len() {
            let pipe_coord = self.pipes[pipe_index].coord;
            let mut segments: Vec<PipeSegmentInfo> = Vec::new();
            for track in &self.pipes[pipe_index].tracks {
                for reference in &track.segments {
                    segments.push(PipeSegmentInfo {
                        edge_index: reference.edge_index,
                        segment_index: reference.segment_index,
                        track_index: track.index,
                        from: reference.from,
                        to: reference.to,
                    });
                }
            }
            segments.sort_by(|a, b| {
                a.from
                    .partial_cmp(&b.from)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let mut clusters: Vec<Vec<PipeSegmentInfo>> = Vec::new();
            if !segments.is_empty() {
                let mut current_cluster: Vec<PipeSegmentInfo> = vec![segments[0]];
                let mut cluster_end = segments[0].to;
                for segment in &segments[1..] {
                    if segment.from < cluster_end {
                        current_cluster.push(*segment);
                        cluster_end = cluster_end.max(segment.to);
                    } else {
                        clusters.push(current_cluster);
                        current_cluster = vec![*segment];
                        cluster_end = segment.to;
                    }
                }
                clusters.push(current_cluster);
            }

            for cluster in clusters {
                let mut used_tracks: Vec<usize> = Vec::new();
                let mut track_scores: HashMap<usize, f64> = HashMap::new();
                for segment in &cluster {
                    if !used_tracks.contains(&segment.track_index) {
                        used_tracks.push(segment.track_index);
                    }
                    let info = self.get_dest_info(segment.edge_index);
                    *track_scores.entry(segment.track_index).or_insert(0.0) += info.delta;
                }

                let mut left_tracks: Vec<usize> = used_tracks
                    .iter()
                    .copied()
                    .filter(|track| track_scores.get(track).copied().unwrap_or(0.0) < -1.0)
                    .collect();
                let mut right_tracks: Vec<usize> = used_tracks
                    .iter()
                    .copied()
                    .filter(|track| track_scores.get(track).copied().unwrap_or(0.0) > 1.0)
                    .collect();
                let mut neutral_tracks: Vec<usize> = used_tracks
                    .iter()
                    .copied()
                    .filter(|track| track_scores.get(track).copied().unwrap_or(0.0).abs() <= 1.0)
                    .collect();

                left_tracks.sort_by(|&a, &b| {
                    let score_b = track_scores.get(&b).copied().unwrap_or(0.0);
                    let score_a = track_scores.get(&a).copied().unwrap_or(0.0);
                    score_b
                        .partial_cmp(&score_a)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                right_tracks.sort_by(|&a, &b| {
                    let score_a = track_scores.get(&a).copied().unwrap_or(0.0);
                    let score_b = track_scores.get(&b).copied().unwrap_or(0.0);
                    score_a
                        .partial_cmp(&score_b)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                let mut left_count = 0;
                for track_index in &left_tracks {
                    left_count += 1;
                    let coord = pipe_coord - left_count as f64 * TRACK_SPACING;
                    assign_track_coords(
                        &cluster,
                        *track_index,
                        coord,
                        pipe_coord,
                        &self.straight_intra_lane_edges,
                        &mut segment_coords,
                    );
                }

                if neutral_tracks.is_empty() && !used_tracks.is_empty() {
                    // `[...usedTracks].sort(...)` keeps the first track on a score tie, so
                    // the scan below only replaces the best on a strictly lower score.
                    let mut best_track = used_tracks[0];
                    let mut best_score =
                        track_scores.get(&best_track).copied().unwrap_or(0.0).abs();
                    for &track in &used_tracks[1..] {
                        let score = track_scores.get(&track).copied().unwrap_or(0.0).abs();
                        if score < best_score {
                            best_track = track;
                            best_score = score;
                        }
                    }
                    if let Some(position) =
                        left_tracks.iter().position(|&track| track == best_track)
                    {
                        left_tracks.remove(position);
                    }
                    if let Some(position) =
                        right_tracks.iter().position(|&track| track == best_track)
                    {
                        right_tracks.remove(position);
                    }
                    neutral_tracks.push(best_track);
                }

                for (neutral_assigned, track_index) in neutral_tracks.iter().enumerate() {
                    if neutral_assigned == 0 {
                        assign_track_coords(
                            &cluster,
                            *track_index,
                            pipe_coord,
                            pipe_coord,
                            &self.straight_intra_lane_edges,
                            &mut segment_coords,
                        );
                    } else {
                        let direction = if neutral_assigned % 2 == 1 { 1.0 } else { -1.0 };
                        let magnitude = (neutral_assigned as f64 / 2.0).ceil();
                        let coord = pipe_coord + direction * magnitude * TRACK_SPACING * 0.5;
                        assign_track_coords(
                            &cluster,
                            *track_index,
                            coord,
                            pipe_coord,
                            &self.straight_intra_lane_edges,
                            &mut segment_coords,
                        );
                    }
                }

                let mut right_count = 0;
                for track_index in &right_tracks {
                    right_count += 1;
                    let coord = pipe_coord + right_count as f64 * TRACK_SPACING;
                    assign_track_coords(
                        &cluster,
                        *track_index,
                        coord,
                        pipe_coord,
                        &self.straight_intra_lane_edges,
                        &mut segment_coords,
                    );
                }
            }
        }

        for edge_index in 0..self.edges.len() {
            let indices = self.edge_segment_indices[edge_index].clone();
            if indices.is_empty() {
                continue;
            }
            let (Some(start), Some(end)) = (
                self.edges[edge_index].start.clone(),
                self.edges[edge_index].end.clone(),
            ) else {
                continue;
            };
            let (Some(src), Some(dst)) = (self.node(&start), self.node(&end)) else {
                continue;
            };
            let (src_port, dst_port) = self.ports_for_edge(edge_index, src, dst);

            let lines: Vec<RoutedLine> = indices
                .iter()
                .map(|&index| {
                    let segment = &self.all_routed_segments[index];
                    let coord = segment_coords
                        .get(&(segment.edge_index, segment.segment_index))
                        .copied()
                        .unwrap_or(segment.pipe_coord);
                    RoutedLine {
                        orient: segment.orientation,
                        coord,
                        from: segment.from,
                        to: segment.to,
                    }
                })
                .collect();

            let mut new_points: Vec<Point> = vec![src_port];
            for k in 0..lines.len() {
                let line = lines[k];
                let previous = *new_points.last().unwrap();
                let previous_along = if line.orient == Orientation::Vertical {
                    previous.y
                } else {
                    previous.x
                };
                let previous_track_coord = if line.orient == Orientation::Vertical {
                    previous.x
                } else {
                    previous.y
                };
                let has_next = k + 1 < lines.len();

                if (previous_track_coord - line.coord).abs() > EPS {
                    new_points.push(point_on_line(line, previous_along));
                }

                if has_next && lines[k + 1].orient == line.orient {
                    let next_line = lines[k + 1];
                    if (line.coord - next_line.coord).abs() > EPS {
                        let junction = if line.orient == Orientation::Vertical {
                            (previous_along + next_line.from) / 2.0
                        } else {
                            shared_line_endpoint_coord(line, next_line)
                        };
                        new_points.push(point_on_line(line, junction));
                        new_points.push(point_on_line(next_line, junction));
                    } else if k == 0 || k == lines.len() - 2 {
                        new_points.push(point_on_line(
                            line,
                            shared_line_endpoint_coord(line, next_line),
                        ));
                    }
                } else if has_next {
                    new_points.push(point_on_line(line, lines[k + 1].coord));
                } else {
                    let end_along =
                        if (line.from - previous_along).abs() < (line.to - previous_along).abs() {
                            line.to
                        } else {
                            line.from
                        };
                    new_points.push(point_on_line(line, end_along));
                }
            }

            let last = *new_points.last().unwrap();
            if (last.x - dst_port.x).abs() > EPS || (last.y - dst_port.y).abs() > EPS {
                new_points.push(dst_port);
            }

            let first = new_points[0];
            if (first.x - src_port.x).abs() > EPS || (first.y - src_port.y).abs() > EPS {
                new_points.insert(0, src_port);
            }

            let mut filtered: Vec<Point> = Vec::new();
            if !new_points.is_empty() {
                filtered.push(new_points[0]);
            }
            for point in &new_points[1..] {
                let previous = filtered[filtered.len() - 1];
                if (point.x - previous.x).abs() > EPS || (point.y - previous.y).abs() > EPS {
                    filtered.push(*point);
                }
            }

            self.edges[edge_index].points = Some(filtered);
        }
    }

    fn write_back(&self, data: &mut LayoutData) {
        for (edge_index, edge) in self.edges.iter().enumerate() {
            if let Some(points) = &edge.points {
                let original = self.original_indices[edge_index];
                data.edges[original].points = Some(points.clone());
            }
        }

        data.edges
            .retain(|edge| !edge.is_layout_only.unwrap_or(false));

        for edge in &mut data.edges {
            let Some(points) = edge.points.as_mut() else {
                continue;
            };
            if points.len() < 2 {
                continue;
            }
            if let Some(src) = edge.start.as_ref().and_then(|id| self.node(id)) {
                points[0] = node_boundary_clamp(points[0], src);
            }
            if let Some(dst) = edge.end.as_ref().and_then(|id| self.node(id)) {
                let last = points.len() - 1;
                points[last] = node_boundary_clamp(points[last], dst);
            }
        }
    }
}

fn assign_lane(
    nodes: &[Node],
    lane_by_node_id: &mut HashMap<String, String>,
    node: &Node,
    lane_id: &str,
) {
    lane_by_node_id.insert(node.id.clone(), lane_id.to_string());
    for child in nodes
        .iter()
        .filter(|child| child.parent_id.as_deref() == Some(node.id.as_str()))
    {
        assign_lane(nodes, lane_by_node_id, child, lane_id);
    }
}

fn segment_ref(segment: &RoutedSegment) -> SegmentRef {
    SegmentRef {
        edge_index: segment.edge_index,
        segment_index: segment.segment_index,
        from: segment.from,
        to: segment.to,
    }
}

#[allow(clippy::too_many_arguments)]
fn assign_track_coords(
    cluster: &[PipeSegmentInfo],
    track_index: usize,
    coord: f64,
    pipe_coord: f64,
    straight_edges: &HashSet<usize>,
    segment_coords: &mut HashMap<(usize, usize), f64>,
) {
    for segment in cluster {
        if segment.track_index != track_index {
            continue;
        }
        let effective = if straight_edges.contains(&segment.edge_index) {
            pipe_coord
        } else {
            coord
        };
        segment_coords.insert((segment.edge_index, segment.segment_index), effective);
    }
}

/// Snap a polyline endpoint onto its node's rectangular boundary, preserving the segment's
/// orientation.
fn node_boundary_clamp(point: Point, node: &Node) -> Point {
    let cx = node.x.unwrap_or(0.0);
    let cy = node.y.unwrap_or(0.0);
    let (width, height) = drawn_size(node, 0.0);
    if width <= 0.0 || height <= 0.0 {
        return point;
    }
    let left = cx - width / 2.0;
    let right = cx + width / 2.0;
    let top = cy - height / 2.0;
    let bottom = cy + height / 2.0;
    if point.x < left || point.x > right || point.y < top || point.y > bottom {
        return point;
    }
    let d_left = point.x - left;
    let d_right = right - point.x;
    let d_top = point.y - top;
    let d_bottom = bottom - point.y;
    let min_d = d_left.min(d_right).min(d_top).min(d_bottom);
    if min_d == d_left {
        return Point::new(left, point.y);
    }
    if min_d == d_right {
        return Point::new(right, point.y);
    }
    if min_d == d_top {
        return Point::new(point.x, top);
    }
    Point::new(point.x, bottom)
}

/// `routeEdgesOrthogonal`: route every layout edge as an orthogonal polyline.
pub fn route_edges_orthogonal(
    data: &mut LayoutData,
    direction: Direction,
) -> Result<(), crate::layout::swimlanes::SwimlaneError> {
    let mut router = Router::new(data, direction);
    router.build_side_info();
    router.split_sibling_sides();
    router.build_port_groups();

    // Incident edge totals for the centered straight-line fast path.
    for edge in &router.edges {
        let (Some(start), Some(end)) = (&edge.start, &edge.end) else {
            continue;
        };
        if start == end {
            continue;
        }
        *router
            .incident_edge_totals
            .entry(start.clone())
            .or_insert(0) += 1;
        *router.incident_edge_totals.entry(end.clone()).or_insert(0) += 1;
    }

    router.run();
    router.write_back(data);
    Ok(())
}
