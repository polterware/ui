//! Source-to-[`Scene`] rendering for block diagrams: the Rust side of upstream
//! `packages/mermaid/src/diagrams/block/blockRenderer.ts` with its `layout.ts` and
//! `renderHelpers.ts`.
//!
//! Block has no `getData()` and does not run `rendering-util/render.ts`: `draw` measures every
//! block by inserting it once, lays the tree out with its own column grid, inserts the blocks
//! again at their computed centres, and then inserts the edges through the shared
//! `rendering-elements/edges.js` code. Here those steps are
//! [`measure_blocks`], [`set_block_sizes`] + [`layout_blocks`] + [`find_bounds`], and
//! [`insert_blocks`] + [`insert_edges`], painted into one [`Scene`].
//!
//! Port notes:
//! * Label measurement uses the crate's [`TextMeasure`] under the same monospace theme the
//!   unified pipeline measures with, so block boxes match the harness's `text-box.mjs` rule.
//! * HTML labels (`foreignObject`) are plain text runs, as everywhere else in this crate.
//! * RoughJS `handDrawn` jitter is not painted; the classic look is what these scenes draw.
//! * The block stylesheet's palette rules (a composite's `data-color-id`) are not carried by
//!   the scene yet, so a composite keeps the theme's default `mainBkg`/`nodeBorder`.

use std::collections::{BTreeMap, HashMap};

use crate::config::Config;
use crate::error::ParseError;
use crate::measure::{Font, TextMeasure, TextOptions, shape_text};
use crate::render::edges::{EdgeData, insert_edge_label, insert_edge_with_type};
use crate::render::intersect::Intersectable;
use crate::render::markers::{marker_definitions, marker_placement};
use crate::render::pipeline::dagre::svg_path_bounds;
use crate::render::shapes::{self, Node as ShapeNode, RenderedNode, ShapeContext, ShapeError};
use crate::scene::{
    Color, Group, Item, Path, PathShape, PathStyle, Point, Rect, RectShape, Scene, Size, SourceRef,
    TextAnchor, TextBaseline, TextRun, Transform,
};
use crate::theme::{Theme, ThemeVariables};

use super::ast::{Ast, Block, BlockType, Edge};
use super::parse;

/// The diagram id `render()` prefixes DOM ids and edge ids with when the caller names none.
pub const DEFAULT_DIAGRAM_ID: &str = "mermaid";

/// The diagram type upstream registers the block renderer under.
pub const DIAGRAM_TYPE: &str = "block";

/// `blockRenderer.ts`'s `markers`.
pub const MARKERS: [&str; 3] = ["point", "circle", "cross"];

/// A block's box in the column grid (`block.size`). `x`/`y` are the centre.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct LayoutSize {
    width: f64,
    height: f64,
    x: f64,
    y: f64,
}

/// The tree `layout.ts` walks. The database's `root` composite is not in `Ast::blocks`.
struct LayoutBlock {
    block: Block,
    columns: Option<i64>,
    width_in_columns: i64,
    children: Vec<LayoutBlock>,
}

impl LayoutBlock {
    fn is_space(&self) -> bool {
        self.block.type_name == BlockType::Space
    }
}

/// What the renderer laid out: the drawn node boxes (spaces omitted, as upstream inserts no
/// DOM for them), the edge paths and their points, and the edge labels.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockLayout {
    pub nodes: Vec<BlockBox>,
    pub edges: Vec<BlockEdgeGeometry>,
    pub labels: Vec<crate::render::edges::EdgeLabelScene>,
}

/// One drawn block shape: the centre the renderer positions it at and the box it paints.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockBox {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// One drawn edge: its upstream id (without the diagram prefix), its points and its path.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockEdgeGeometry {
    pub id: String,
    pub points: Vec<Point>,
    pub path: Path,
}

/// Renders a block body (the text [`super::parse`] accepts) to a [`Scene`].
pub fn draw(
    body: &str,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, BlockRenderError> {
    let ast = parse(body)?;
    draw_ast(&ast, config, measure)
}

/// Renders an already parsed block diagram.
pub fn draw_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, BlockRenderError> {
    draw_layout_ast(ast, config, measure).map(|(_, scene)| scene)
}

/// Renders an already parsed block diagram and keeps the layout the scene was painted from.
pub fn draw_layout_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<(BlockLayout, Scene), BlockRenderError> {
    let tree = build_tree(ast);
    let padding = config.block.padding;
    let theme = measurement_theme();
    let monospace = Font {
        family: theme.color("fontFamily").unwrap_or("monospace").to_string(),
        size: theme
            .color("fontSize")
            .and_then(|value| value.trim_end_matches("px").parse::<f64>().ok())
            .or_else(|| theme.number("fontSize"))
            .unwrap_or(16.0),
        weight: crate::scene::NORMAL_WEIGHT,
        italic: false,
    };

    let mut sizes = HashMap::new();
    measure_blocks(&tree, config, measure, &theme, &mut sizes)?;
    // `setBlockSizes(root, db, 0, 0, padding)`: root has no measured size.
    sizes.insert("root".into(), LayoutSize::default());
    set_block_sizes(&tree, &mut sizes, 0.0, 0.0, padding)?;
    layout_blocks(&tree, &mut sizes, padding)?;

    let mut scene = Scene::default();
    for def in marker_definitions(DEFAULT_DIAGRAM_ID, DIAGRAM_TYPE, &MARKERS) {
        scene.define(def);
    }

    let mut layout = BlockLayout::default();
    let mut node_bounds: Option<Rect> = None;
    let mut intersections: HashMap<String, ShapeNode> = HashMap::new();
    insert_blocks(
        &tree,
        &sizes,
        config,
        measure,
        &theme,
        &mut scene,
        &mut layout,
        &mut node_bounds,
        &mut intersections,
    )?;

    let mut path_bounds: Option<Rect> = None;
    insert_edges(
        ast,
        &sizes,
        &intersections,
        measure,
        &theme,
        &monospace,
        &mut scene,
        &mut layout,
        &mut path_bounds,
    )?;

    // `draw` asks the inserted group for its `getBBox()` and falls back to the layout bounds
    // only when that box is not finite. The scene gets the same union of what it paints: the
    // node shapes, the edge paths and the edge label backgrounds.
    let layout_bounds = find_bounds(&tree, &sizes);
    let mut bounds = node_bounds.unwrap_or(layout_bounds);
    if let Some(path) = path_bounds {
        bounds = bounds.union(path);
    }
    for label in &layout.labels {
        if let Some(background) = label.background {
            bounds = bounds.union(background);
        }
    }

    scene.bounds = bounds;
    scene.view_box = Rect::new(
        bounds.x - 5.0,
        bounds.y - 5.0,
        bounds.width + 10.0,
        bounds.height + 10.0,
    );
    Ok((layout, scene))
}

/// Renders a block body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, BlockRenderError> {
    draw(body, config, &crate::measure::FallbackMeasure)
}

/// The theme the unified pipeline hands to shape measurement (`dagre.rs`): monospace at 16px,
/// the same rule as the harness's `text-box.mjs`.
fn measurement_theme() -> ThemeVariables {
    let mut overrides = serde_json::Map::new();
    overrides.insert("fontFamily".into(), serde_json::json!("monospace"));
    overrides.insert("fontSize".into(), serde_json::json!("16px"));
    Theme::Default.variables(&overrides).unwrap_or_default()
}

/// What can stop a block render.
#[derive(Clone, Debug, PartialEq)]
pub enum BlockRenderError {
    Parse(ParseError),
    Shape(String),
    Layout(String),
}

impl std::fmt::Display for BlockRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => error.fmt(f),
            Self::Shape(message) => f.write_str(message),
            Self::Layout(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for BlockRenderError {}

impl From<ParseError> for BlockRenderError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl From<ShapeError> for BlockRenderError {
    fn from(error: ShapeError) -> Self {
        Self::Shape(error.message)
    }
}

fn build_tree(ast: &Ast) -> LayoutBlock {
    let root = ast.block("root");
    let columns = root.and_then(|block| block.columns);
    LayoutBlock {
        block: Block {
            id: "root".into(),
            label: String::new(),
            type_name: BlockType::Composite,
            columns,
            width_in_columns: None,
            width: None,
            classes: Vec::new(),
            directions: Vec::new(),
            styles: Vec::new(),
            color_index: None,
            children: Some(ast.blocks.clone()),
        },
        columns,
        width_in_columns: 1,
        children: ast.blocks.iter().map(build_layout_block).collect(),
    }
}

fn build_layout_block(block: &Block) -> LayoutBlock {
    LayoutBlock {
        block: block.clone(),
        columns: block.columns,
        width_in_columns: block.width_in_columns.unwrap_or(1),
        children: block
            .children
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(build_layout_block)
            .collect(),
    }
}

/// `calculateBlockSize` over `getBlocks()`: measure each block in insertion order, parents
/// before children, and keep the shape the edge pass intersects.
fn measure_blocks(
    tree: &LayoutBlock,
    config: &Config,
    measure: &dyn TextMeasure,
    theme: &ThemeVariables,
    sizes: &mut HashMap<String, LayoutSize>,
) -> Result<(), BlockRenderError> {
    for child in &tree.children {
        if child.block.type_name != BlockType::Group {
            let node = block_node(child, false, LayoutSize::default(), config);
            let rendered = draw_block(&node, &child.block, measure, theme)?;
            sizes.insert(
                child.block.id.clone(),
                LayoutSize {
                    width: rendered.node.bounds.width,
                    height: rendered.node.bounds.height,
                    x: 0.0,
                    y: 0.0,
                },
            );
        }
        measure_blocks(child, config, measure, theme, sizes)?;
    }
    Ok(())
}

/// `getNodeFromBlock` plus `insertNode`: the shape handler for a block, measured (`positioned`
/// false) or painted at its laid-out size (`positioned` true).
fn block_node(
    block: &LayoutBlock,
    positioned: bool,
    size: LayoutSize,
    config: &Config,
) -> ShapeNode {
    let block = &block.block;
    let class_str = if block.classes.is_empty() {
        "default".to_string()
    } else {
        block.classes.join(" ")
    };
    let radius = if block.type_name == BlockType::Round {
        5.0
    } else {
        0.0
    };
    let (shape, padding) = match block.type_name {
        BlockType::Round => ("rect", config.block.padding),
        BlockType::Composite => ("composite", 0.0),
        BlockType::Square => ("rect", config.block.padding),
        BlockType::Diamond => ("question", config.block.padding),
        BlockType::Hexagon => ("hexagon", config.block.padding),
        BlockType::BlockArrow => ("block_arrow", config.block.padding),
        BlockType::Odd => ("rect_left_inv_arrow", config.block.padding),
        BlockType::LeanRight => ("lean_right", config.block.padding),
        BlockType::LeanLeft => ("lean_left", config.block.padding),
        BlockType::Trapezoid => ("trapezoid", config.block.padding),
        BlockType::InvTrapezoid => ("inv_trapezoid", config.block.padding),
        BlockType::RectLeftInvArrow => ("rect_left_inv_arrow", config.block.padding),
        BlockType::Circle => ("circle", config.block.padding),
        BlockType::Ellipse => ("ellipse", config.block.padding),
        BlockType::Stadium => ("stadium", config.block.padding),
        BlockType::Subroutine => ("subroutine", config.block.padding),
        BlockType::Cylinder => ("cylinder", config.block.padding),
        BlockType::Group => ("rect", config.block.padding),
        BlockType::DoubleCircle => ("doublecircle", config.block.padding),
        _ => ("rect", config.block.padding),
    };
    let mut node = ShapeNode::new(block.id.clone(), block.label.clone(), shape);
    node.css_classes = format!("{class_str} flowchart-label");
    node.css_styles = block.styles.clone();
    node.rx = radius;
    node.ry = radius;
    node.padding = padding;
    node.look = config.look.clone();
    node.wrapping_width = Some(if positioned && size.width != 0.0 {
        size.width
    } else {
        f64::INFINITY
    });
    node.width = (positioned && size.width != 0.0).then_some(size.width);
    node.height = (positioned && size.height != 0.0).then_some(size.height);
    node.positioned = positioned;
    node
}

fn draw_block(
    node: &ShapeNode,
    block: &Block,
    measure: &dyn TextMeasure,
    theme: &ThemeVariables,
) -> Result<RenderedNode, BlockRenderError> {
    let ctx = ShapeContext { measure, theme };
    if block.type_name == BlockType::BlockArrow {
        let directions: Vec<&str> = block
            .directions
            .iter()
            .map(|direction| direction.trim())
            .collect();
        Ok(shapes::block_arrow::draw_with_directions(
            node,
            &ctx,
            &directions,
        )?)
    } else {
        Ok(shapes::render_node(node.clone(), measure, theme)?)
    }
}

fn width_in_columns(block: &LayoutBlock) -> i64 {
    block.width_in_columns
}

/// `getMaxChildSize`: the widest normalized child and the tallest child, skipping spaces.
fn get_max_child_size(block: &LayoutBlock, sizes: &HashMap<String, LayoutSize>) -> (f64, f64) {
    let mut max_width = 0.0;
    let mut max_height = 0.0;
    for child in &block.children {
        if child.is_space() {
            continue;
        }
        let size = sizes.get(&child.block.id).copied().unwrap_or_default();
        let normalized = size.width / width_in_columns(child) as f64;
        if normalized > max_width {
            max_width = normalized;
        }
        if size.height > max_height {
            max_height = size.height;
        }
    }
    (max_width, max_height)
}

/// `calculateBlockPosition`: `(column, row)` for a statement's position.
fn calculate_block_position(columns: i64, position: i64) -> Result<(i64, i64), BlockRenderError> {
    if columns == 0 {
        return Err(BlockRenderError::Layout(
            "Columns must be an integer !== 0.".into(),
        ));
    }
    if position < 0 {
        return Err(BlockRenderError::Layout(format!(
            "Position must be a non-negative integer.{position}"
        )));
    }
    if columns < 0 {
        return Ok((position, 0));
    }
    if columns == 1 {
        return Ok((0, position));
    }
    Ok((position % columns, position / columns))
}

/// `setBlockSizes`: bottom-up sizing, then stretching rows and children to fill a sibling slot.
fn set_block_sizes(
    block: &LayoutBlock,
    sizes: &mut HashMap<String, LayoutSize>,
    sibling_width: f64,
    sibling_height: f64,
    padding: f64,
) -> Result<(), BlockRenderError> {
    {
        let size = sizes.entry(block.block.id.clone()).or_default();
        if size.width == 0.0 {
            *size = LayoutSize {
                width: sibling_width,
                height: sibling_height,
                x: 0.0,
                y: 0.0,
            };
        }
    }

    if block.children.is_empty() {
        return Ok(());
    }

    for child in &block.children {
        set_block_sizes(child, sizes, 0.0, 0.0, padding)?;
    }
    let (max_width, max_height) = get_max_child_size(block, sizes);

    for child in &block.children {
        let columns = width_in_columns(child) as f64;
        if let Some(size) = sizes.get_mut(&child.block.id) {
            size.width = max_width * columns + padding * (columns - 1.0);
            size.height = max_height;
            size.x = 0.0;
            size.y = 0.0;
        }
    }
    for child in &block.children {
        set_block_sizes(child, sizes, max_width, max_height, padding)?;
    }

    let columns = block.columns.unwrap_or(-1);
    let num_items: i64 = block.children.iter().map(width_in_columns).sum();
    let mut x_size = block.children.len() as i64;
    if columns > 0 && columns < num_items {
        x_size = columns;
    }
    let y_size = (num_items as f64 / x_size as f64).ceil() as i64;

    let mut width = x_size as f64 * (max_width + padding) + padding;
    let mut height = y_size as f64 * (max_height + padding) + padding;

    if width < sibling_width {
        width = sibling_width;
        height = sibling_height;
        let child_width = (sibling_width - x_size as f64 * padding - padding) / x_size as f64;
        let child_height = (sibling_height - y_size as f64 * padding - padding) / y_size as f64;
        for child in &block.children {
            if let Some(size) = sizes.get_mut(&child.block.id) {
                size.width = child_width;
                size.height = child_height;
                size.x = 0.0;
                size.y = 0.0;
            }
        }
    }

    if width
        < sizes
            .get(&block.block.id)
            .map(|size| size.width)
            .unwrap_or(0.0)
    {
        width = sizes
            .get(&block.block.id)
            .map(|size| size.width)
            .unwrap_or(0.0);
        let num = if columns > 0 {
            block.children.len().min(columns as usize)
        } else {
            block.children.len()
        };
        if num > 0 {
            let child_width = (width - num as f64 * padding - padding) / num as f64;
            for child in &block.children {
                if let Some(size) = sizes.get_mut(&child.block.id) {
                    size.width = child_width;
                }
            }
        }
    }

    if let Some(size) = sizes.get_mut(&block.block.id) {
        *size = LayoutSize {
            width,
            height,
            x: 0.0,
            y: 0.0,
        };
    }
    Ok(())
}

/// `layoutBlocks`: centres each child inside its parent's column grid.
fn layout_blocks(
    block: &LayoutBlock,
    sizes: &mut HashMap<String, LayoutSize>,
    padding: f64,
) -> Result<(), BlockRenderError> {
    if block.children.is_empty() {
        return Ok(());
    }
    let columns = block.columns.unwrap_or(-1);

    // Pre-compute per-row max heights so y-positioning accounts for rows of different heights.
    let mut row_heights: BTreeMap<i64, f64> = BTreeMap::new();
    {
        let mut column_pos = 0i64;
        for child in &block.children {
            let Some(size) = sizes.get(&child.block.id) else {
                continue;
            };
            let (_, row) = calculate_block_position(columns, column_pos)?;
            let current = row_heights.get(&row).copied().unwrap_or(0.0);
            if size.height > current {
                row_heights.insert(row, size.height);
            }
            let mut filled = width_in_columns(child);
            if columns > 0 {
                filled = filled.min(columns - column_pos % columns);
            }
            column_pos += filled;
        }
    }
    let mut row_y_offsets: BTreeMap<i64, f64> = BTreeMap::new();
    {
        let mut offset = 0.0;
        for (row, height) in &row_heights {
            row_y_offsets.insert(*row, offset);
            offset += height + padding;
        }
    }

    let parent = sizes.get(&block.block.id).copied().unwrap_or_default();
    let mut column_pos = 0i64;
    let mut starting_pos_x = if parent.x != 0.0 {
        parent.x - parent.width / 2.0
    } else {
        -padding
    };
    let mut row_pos = 0i64;

    for child in &block.children {
        let Some(child_size) = sizes.get(&child.block.id).copied() else {
            continue;
        };
        let (_, row) = calculate_block_position(columns, column_pos)?;
        if row != row_pos {
            row_pos = row;
            starting_pos_x = if parent.x != 0.0 {
                parent.x - parent.width / 2.0
            } else {
                -padding
            };
        }

        let half_width = child_size.width / 2.0;
        let x = starting_pos_x + padding + half_width;
        starting_pos_x = x + half_width;
        let row_y_offset = row_y_offsets.get(&row).copied().unwrap_or(0.0);
        let row_height = row_heights.get(&row).copied().unwrap_or(child_size.height);
        let y = parent.y - parent.height / 2.0 + row_y_offset + row_height / 2.0 + padding;

        if let Some(size) = sizes.get_mut(&child.block.id) {
            size.x = x;
            size.y = y;
        }

        // `layoutBlocks` recurses even for the child whose size was just read; its own grid
        // starts once its centre is known.
        layout_blocks(child, sizes, padding)?;

        let mut filled = width_in_columns(child);
        if columns > 0 {
            filled = filled.min(columns - column_pos % columns);
        }
        column_pos += filled;
    }

    Ok(())
}

/// `findBounds`: the union of every sized block except the root composite.
fn find_bounds(tree: &LayoutBlock, sizes: &HashMap<String, LayoutSize>) -> Rect {
    struct Bounds {
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
    }
    fn walk(block: &LayoutBlock, sizes: &HashMap<String, LayoutSize>, bounds: &mut Bounds) {
        if block.block.id != "root"
            && let Some(size) = sizes.get(&block.block.id)
        {
            bounds.min_x = bounds.min_x.min(size.x - size.width / 2.0);
            bounds.min_y = bounds.min_y.min(size.y - size.height / 2.0);
            bounds.max_x = bounds.max_x.max(size.x + size.width / 2.0);
            bounds.max_y = bounds.max_y.max(size.y + size.height / 2.0);
        }
        for child in &block.children {
            walk(child, sizes, bounds);
        }
    }

    let mut bounds = Bounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 0.0,
        max_y: 0.0,
    };
    for child in &tree.children {
        walk(child, sizes, &mut bounds);
    }
    Rect::new(
        bounds.min_x,
        bounds.min_y,
        bounds.max_x - bounds.min_x,
        bounds.max_y - bounds.min_y,
    )
}

/// The box a positioned node group paints, transforms applied: the `getBBox()` upstream asks
/// the inserted `<g>` for when it fits the viewBox. A block arrow's path is not centred on the
/// block centre, so the shape's own `node.bounds` is not enough.
fn group_bounds(group: &Group) -> Option<Rect> {
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
                if let Some(bounds) = svg_path_bounds(&shape.path) {
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
            // The node's label sits inside its shape, and the scene's text box uses the
            // painter's baseline model rather than the browser `getBBox` the harness measures;
            // the upstream box is the shape, so text does not widen it here.
            Item::Text(_) => {}
            Item::Image(image) => include(out, transformed(image.rect, transform)),
        }
    }

    let mut bounds = None;
    walk(
        &Item::Group(group.clone()),
        Transform::IDENTITY,
        &mut bounds,
    );
    bounds
}

/// `insertBlocks`: insert the measured block (unless it is a space) at its laid-out centre,
/// then recurse into its children.
#[allow(clippy::too_many_arguments)]
fn insert_blocks(
    tree: &LayoutBlock,
    sizes: &HashMap<String, LayoutSize>,
    config: &Config,
    measure: &dyn TextMeasure,
    theme: &ThemeVariables,
    scene: &mut Scene,
    layout: &mut BlockLayout,
    node_bounds: &mut Option<Rect>,
    intersections: &mut HashMap<String, ShapeNode>,
) -> Result<(), BlockRenderError> {
    for child in &tree.children {
        let Some(size) = sizes.get(&child.block.id).copied() else {
            continue;
        };
        if !child.is_space() {
            let node = block_node(child, true, size, config);
            let mut rendered = draw_block(&node, &child.block, measure, theme)?;
            shapes::position_node(
                &mut rendered.group,
                &rendered.node,
                size.x,
                size.y,
                false,
                0.0,
            );
            if let Some(bounds) = group_bounds(&rendered.group) {
                *node_bounds = Some(match *node_bounds {
                    Some(current) => current.union(bounds),
                    None => bounds,
                });
            }
            layout.nodes.push(BlockBox {
                id: child.block.id.clone(),
                x: size.x,
                y: size.y,
                width: rendered.node.bounds.width,
                height: rendered.node.bounds.height,
            });
            intersections.insert(child.block.id.clone(), rendered.node.clone());
            scene.push(rendered.group);
        }
        insert_blocks(
            child,
            sizes,
            config,
            measure,
            theme,
            scene,
            layout,
            node_bounds,
            intersections,
        )?;
    }
    Ok(())
}

/// `insertEdges`: the raw `start → midpoint → end` route, clipped against the block shapes,
/// then the shared `insertEdge` path and label builders.
#[allow(clippy::too_many_arguments)]
fn insert_edges(
    ast: &Ast,
    sizes: &HashMap<String, LayoutSize>,
    intersections: &HashMap<String, ShapeNode>,
    measure: &dyn TextMeasure,
    theme: &ThemeVariables,
    font: &Font,
    scene: &mut Scene,
    layout: &mut BlockLayout,
    path_bounds: &mut Option<Rect>,
) -> Result<(), BlockRenderError> {
    let line_color = theme.color("lineColor").unwrap_or("#333333").to_string();
    for edge in &ast.edges {
        if edge.start.is_empty() || edge.end.is_empty() {
            continue;
        }
        let Some(start_size) = sizes.get(&edge.start) else {
            continue;
        };
        let Some(end_size) = sizes.get(&edge.end) else {
            continue;
        };
        let start = Point::new(start_size.x, start_size.y);
        let end = Point::new(end_size.x, end_size.y);
        let middle = Point::new(
            start.x + (end.x - start.x) / 2.0,
            start.y + (end.y - start.y) / 2.0,
        );
        let mut points = vec![start, middle, end];

        // `tail.intersect(points[0])` / `head.intersect(points[0])` where the inner slice's
        // first point is the midpoint. Spaces have no inserted shape, so their intersect is
        // undefined and upstream leaves the raw route untouched.
        let tail = intersections.get(&edge.start);
        let head = intersections.get(&edge.end);
        if let (Some(tail), Some(head)) = (tail, head) {
            points[0] = intersect_block(tail, start, middle);
            points[2] = intersect_block(head, end, middle);
        }

        let label_size = if edge.label.is_empty() {
            None
        } else {
            let block = shape_text(
                &edge.label,
                &TextOptions {
                    width: f64::INFINITY,
                    markdown: false,
                    min_width: 0.0,
                },
                font,
                measure,
            )
            .map_err(|error| BlockRenderError::Shape(error.to_string()))?;
            Some(block.size)
        };

        let mut data = edge_data(edge, points.clone(), label_size, middle);
        let inserted = insert_edge_with_type(&data, DEFAULT_DIAGRAM_ID, DIAGRAM_TYPE)?;

        let stroke_width = if edge.thickness == "thick" { 3.5 } else { 1.0 };
        let mut path_shape = PathShape::new(
            inserted.path.clone(),
            PathStyle::stroked(Color::new(line_color.clone()), stroke_width),
        )
        .with_source(SourceRef::edge(edge.id.clone()));
        if let Ok(placement) = marker_placement(
            Some(edge.arrow_type_start.as_str()),
            Some(edge.arrow_type_end.as_str()),
            DEFAULT_DIAGRAM_ID,
            DIAGRAM_TYPE,
            false,
        ) {
            if let Some(marker) = placement.start {
                path_shape.start_markers.push(marker);
            }
            if let Some(marker) = placement.end {
                path_shape.end_markers.push(marker);
            }
        }
        scene.push(path_shape);

        layout.edges.push(BlockEdgeGeometry {
            id: edge.id.clone(),
            points: inserted.points.clone(),
            path: inserted.path.clone(),
        });
        if let Some(bounds) = svg_path_bounds(&inserted.path) {
            *path_bounds = Some(match *path_bounds {
                Some(current) => current.union(bounds),
                None => bounds,
            });
        }

        if let Some(label_size) = label_size {
            data.label_width = label_size.width;
            data.label_height = label_size.height;
            data.label_x = Some(middle.x);
            data.label_y = Some(middle.y);
            for label in insert_edge_label(&data) {
                push_edge_label(&label, theme, font, measure, scene)?;
                layout.labels.push(label);
            }
        }
    }
    Ok(())
}

fn edge_data(edge: &Edge, points: Vec<Point>, label_size: Option<Size>, middle: Point) -> EdgeData {
    EdgeData {
        id: edge.id.clone(),
        points,
        label: if edge.label.is_empty() {
            None
        } else {
            Some(edge.label.clone())
        },
        label_width: label_size.map(|size| size.width).unwrap_or(0.0),
        label_height: label_size.map(|size| size.height).unwrap_or(0.0),
        label_x: label_size.map(|_| middle.x),
        label_y: label_size.map(|_| middle.y),
        curve: None,
        thickness: edge.thickness.clone(),
        pattern: edge.pattern.clone(),
        arrow_type_start: Some(edge.arrow_type_start.clone()),
        arrow_type_end: Some(edge.arrow_type_end.clone()),
        look: Some("classic".into()),
        skip_intersect: true,
        ..Default::default()
    }
}

/// Intersects a block's local shape outline after translating the query point into the
/// block's centred coordinate space.
fn intersect_block(node: &ShapeNode, center: Point, point: Point) -> Point {
    let local = Point::new(point.x - center.x, point.y - center.y);
    let hit = node.intersect(local);
    Point::new(hit.x + center.x, hit.y + center.y)
}

fn push_edge_label(
    label: &crate::render::edges::EdgeLabelScene,
    theme: &ThemeVariables,
    font: &Font,
    measure: &dyn TextMeasure,
    scene: &mut Scene,
) -> Result<(), BlockRenderError> {
    let block = shape_text(
        &label.text,
        &TextOptions {
            width: f64::INFINITY,
            markdown: false,
            min_width: 0.0,
        },
        font,
        measure,
    )
    .map_err(|error| BlockRenderError::Shape(error.to_string()))?;
    let mut run = TextRun::new(Point::ZERO, font.clone(), block);
    run.anchor = TextAnchor::Middle;
    run.baseline = TextBaseline::Middle;
    run.fill = theme
        .color("textColor")
        .map(|color| Color::new(color.to_string()));

    let mut group = Group::new().with_class("edgeLabel");
    group.transform = Transform::translate(label.position.center.x, label.position.center.y);
    if let Some(background) = label.background {
        let local = Rect::new(
            background.x - label.position.center.x,
            background.y - label.position.center.y,
            background.width,
            background.height,
        );
        let fill = theme
            .color("edgeLabelBackground")
            .unwrap_or("rgba(232,232,232, 0.8)")
            .to_string();
        group.push(RectShape::new(
            local,
            0.0,
            PathStyle::filled(Color::new(fill)).with_opacity(0.5),
        ));
    }
    group.push(Item::Text(run));
    scene.push(group);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::measure::FallbackMeasure;
    use crate::scene::PathCommand;
    use serde_json::Value;

    const TOLERANCE: f64 = 0.01;

    fn fixtures() -> Vec<(String, Value)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/diagrams/block/tests/render");
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

    fn close_box(actual: (f64, f64, f64, f64), expected: &Value, what: &str, file: &str) {
        let expected = (
            expected["x"].as_f64().expect("x"),
            expected["y"].as_f64().expect("y"),
            expected["width"].as_f64().expect("width"),
            expected["height"].as_f64().expect("height"),
        );
        close(actual.0, expected.0, &format!("{what} x"), file);
        close(actual.1, expected.1, &format!("{what} y"), file);
        close(actual.2, expected.2, &format!("{what} width"), file);
        close(actual.3, expected.3, &format!("{what} height"), file);
    }

    fn parse_path(d: &str) -> Vec<(char, Vec<f64>)> {
        let mut commands = Vec::new();
        let mut current: Option<(char, Vec<f64>)> = None;
        let mut number = String::new();
        let flush_number = |current: &mut Option<(char, Vec<f64>)>, number: &mut String| {
            if number.is_empty() {
                return;
            }
            if let Ok(value) = number.parse::<f64>()
                && let Some((_, values)) = current.as_mut()
            {
                values.push(value);
            }
            number.clear();
        };
        for character in d.chars() {
            if character.is_ascii_alphabetic()
                && (number.is_empty() || !matches!(character, 'e' | 'E'))
            {
                flush_number(&mut current, &mut number);
                if let Some(command) = current.take() {
                    commands.push(command);
                }
                current = Some((character.to_ascii_uppercase(), Vec::new()));
            } else if character == ',' || character.is_whitespace() {
                flush_number(&mut current, &mut number);
            } else {
                number.push(character);
            }
        }
        flush_number(&mut current, &mut number);
        if let Some(command) = current.take() {
            commands.push(command);
        }
        commands
    }

    fn scene_path(path: &Path) -> Vec<(char, Vec<f64>)> {
        path.commands
            .iter()
            .map(|command| match *command {
                PathCommand::MoveTo { x, y } => ('M', vec![x, y]),
                PathCommand::LineTo { x, y } => ('L', vec![x, y]),
                PathCommand::QuadTo { cx, cy, x, y } => ('Q', vec![cx, cy, x, y]),
                PathCommand::CubicTo {
                    c1x,
                    c1y,
                    c2x,
                    c2y,
                    x,
                    y,
                } => ('C', vec![c1x, c1y, c2x, c2y, x, y]),
                PathCommand::ArcTo {
                    rx,
                    ry,
                    sweep,
                    x,
                    y,
                    ..
                } => ('A', vec![rx, ry, f64::from(u8::from(sweep)), x, y]),
                PathCommand::Close => ('Z', Vec::new()),
            })
            .collect()
    }

    fn compare_path(actual: &Path, expected: &str, what: &str, file: &str) {
        let expected = parse_path(expected);
        let actual = scene_path(actual);
        assert_eq!(
            actual
                .iter()
                .map(|(command, _)| *command)
                .collect::<Vec<_>>(),
            expected
                .iter()
                .map(|(command, _)| *command)
                .collect::<Vec<_>>(),
            "{file}: {what}: path commands"
        );
        for (index, ((_, actual_values), (_, expected_values))) in
            actual.iter().zip(expected.iter()).enumerate()
        {
            assert_eq!(
                actual_values.len(),
                expected_values.len(),
                "{file}: {what}: command {index} arity"
            );
            for (value_index, (actual_value, expected_value)) in
                actual_values.iter().zip(expected_values.iter()).enumerate()
            {
                close(
                    *actual_value,
                    *expected_value,
                    &format!("{what} command {index} value {value_index}"),
                    file,
                );
            }
        }
    }

    #[test]
    fn renders_recorded_block_fixtures() {
        let fixtures = fixtures();
        assert!(
            fixtures.len() >= 8,
            "expected at least 8 fixtures, found {}",
            fixtures.len()
        );

        for (file, fixture) in fixtures {
            let source = fixture["source"].as_str().expect("fixture source");
            let body = crate::preprocess::preprocess(source).body;
            let ast =
                super::super::parse(&body).unwrap_or_else(|error| panic!("{file}: parse: {error}"));
            let (layout, scene) = draw_layout_ast(&ast, &Config::default(), &FallbackMeasure)
                .unwrap_or_else(|error| panic!("{file}: render: {error}"));
            let expected = &fixture["expected"];

            for node in expected["nodes"].as_array().expect("expected nodes") {
                let id = node["id"].as_str().expect("node id");
                let actual = layout
                    .nodes
                    .iter()
                    .find(|candidate| candidate.id == id)
                    .unwrap_or_else(|| panic!("{file}: node {id} in layout"));
                close_box(
                    (actual.x, actual.y, actual.width, actual.height),
                    node,
                    &format!("node {id}"),
                    &file,
                );
            }

            for edge in expected["edges"].as_array().expect("expected edges") {
                let id = edge["id"].as_str().expect("edge id");
                let actual = layout
                    .edges
                    .iter()
                    .find(|candidate| candidate.id == id)
                    .unwrap_or_else(|| panic!("{file}: edge {id} in layout"));
                let expected_points = edge["points"].as_array().expect("points");
                assert_eq!(
                    actual.points.len(),
                    expected_points.len(),
                    "{file}: edge {id} point count"
                );
                for (index, (actual_point, expected_point)) in
                    actual.points.iter().zip(expected_points).enumerate()
                {
                    close(
                        actual_point.x,
                        expected_point["x"].as_f64().expect("point x"),
                        &format!("edge {id} point {index} x"),
                        &file,
                    );
                    close(
                        actual_point.y,
                        expected_point["y"].as_f64().expect("point y"),
                        &format!("edge {id} point {index} y"),
                        &file,
                    );
                }
                if let Some(d) = edge["d"].as_str().filter(|d| !d.is_empty()) {
                    compare_path(&actual.path, d, &format!("edge {id}"), &file);
                }
            }

            for label in expected["labels"].as_array().expect("expected labels") {
                let text = label["text"].as_str().expect("label text");
                let actual = layout
                    .labels
                    .iter()
                    .find(|candidate| candidate.text.contains(text.trim()))
                    .unwrap_or_else(|| panic!("{file}: label {text:?} in layout"));
                let background = actual.background.expect("label background");
                close_box(
                    (
                        background.x,
                        background.y,
                        background.width,
                        background.height,
                    ),
                    label,
                    &format!("label {text:?}"),
                    &file,
                );
            }

            let expected_view_box = &expected["viewBox"];
            close(
                scene.view_box.x,
                expected_view_box["x"].as_f64().expect("viewBox x"),
                "viewBox x",
                &file,
            );
            close(
                scene.view_box.y,
                expected_view_box["y"].as_f64().expect("viewBox y"),
                "viewBox y",
                &file,
            );
            close(
                scene.view_box.width,
                expected_view_box["width"].as_f64().expect("viewBox width"),
                "viewBox width",
                &file,
            );
            close(
                scene.view_box.height,
                expected_view_box["height"]
                    .as_f64()
                    .expect("viewBox height"),
                "viewBox height",
                &file,
            );
        }
    }

    #[test]
    fn draws_a_rendered_scene_with_view_box() {
        let scene = render(
            "block-beta\n  columns 2\n  a\n  b\n  a --> b\n",
            &Config::default(),
        )
        .expect("render block diagram");
        assert!(scene.view_box.width > 0.0);
        assert!(scene.view_box.height > 0.0);
        assert!(!scene.items.is_empty());
    }

    #[test]
    fn rejects_a_body_without_a_block_header() {
        let error = render("flowchart TD\n  A --> B\n", &Config::default()).unwrap_err();
        assert!(matches!(error, BlockRenderError::Parse(_)));
    }
}
