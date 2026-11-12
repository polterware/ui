//! Source-to-[`Scene`] rendering for tree views: the Rust side of
//! `diagrams/treeView/renderer.ts` `draw`.
//!
//! A tree view does not go through the unified layout pipeline: `draw` uses plain d3 and the
//! DOM. It walks the database's synthetic root in pre-order and, for every node, appends a
//! `<g>` with the node's label, draws a short horizontal branch line, recurses into the
//! children, and then draws one vertical line from the node down to its last child's centre.
//! A second pass appends each description at a common column (`max(labelRightEdge) + 16`), and
//! a third widens each `highlight` background rectangle to the tree's full width. The
//! viewport is `-lineThickness / 2, 0, totalWidth, totalHeight`.
//!
//! Look: `diagrams/treeView/styles.ts`, which is the whole visual definition — a 16px label,
//! black labels, bold directories, black 1px connector lines, `#546e7a` icons, italic
//! `#6a9955` descriptions, and the amber `rgba(255, 193, 7, 0.15)` / `#ffc107` highlight.
//! The design system has no tree-view control, so the look is mermaid's own.
//!
//! Behavior: gpui-kit's `Tree` (`TreeItem`/`TreeState`) is the closest counterpart — rows in
//! pre-order carrying a depth and a stable id, indented by depth — and this module keeps that
//! flattening. Upstream draws every row open and has no selection or expand state, so there is
//! nothing to port beyond the walk itself; `renderer.ts` is followed call for call.
//!
//! Where the port stops short of upstream, the difference is in the painter-facing scene:
//!
//! * The class names (`treeView-node-label`, `treeView-node-dir`, `treeView-node-line`,
//!   `treeView-node-description`, `treeView-highlight-bg`) matter only to CSS, and a [`Scene`]
//!   carries paints rather than selectors, so `draw` resolves `styles.ts`'s declarations into
//!   the runs' fonts and fills and the shapes' strokes. The `tree-view` group's class is kept.
//! * Icons are inline HTML SVGs from iconify. This crate has no icon pack registry, so only the
//!   built-in `mermaid-treeview` pack (`file`, `folder`) resolves; anything else draws
//!   upstream's `unknownIcon`, the blue box with a `?`, which is what `getIconSVG`'s `catch`
//!   does for an unregistered pack. The nested `<svg width="14" height="14" viewBox="…">` is a
//!   group that translates to the icon's corner and scales by `14 / viewBoxSize`.
//! * `useMaxWidth` sizes the `<svg>` element, not the drawing; the scene carries the viewBox
//!   `draw` sets and the painter sizes the page.
//! * Text is measured in the `monospace` family, the rule the fixture harness renders with and
//!   that [`FallbackMeasure`](crate::measure::FallbackMeasure) reproduces. Bold directories and
//!   italic descriptions are measured in the upright normal-weight face, because a browser's
//!   monospace keeps its advance under bold and italic; the drawing font still carries the
//!   weight and slant. An empty label measures zero high, as the harness's `getBBox` does.
//! * Nothing draws the diagram title or the accessibility strings; `draw` ignores them, and so
//!   does the scene.

use crate::config::{Config, TreeViewConfig};
use crate::error::ParseError;
use crate::measure::{FallbackMeasure, Font, TextMeasure};
use crate::render::shapes::bpmn_icons::parse_path_data;
use crate::scene::{
    Color, FillRule, Group, Path, PathShape, PathStyle, Point, Rect, RectShape, Scene, Size,
    TextBaseline, TextBlock, TextLine, TextRun, TextSpan, Transform,
};

use super::Ast;
use super::ast::{Node, NodeType};

/// `renderer.ts`'s `ICON_SIZE`.
pub const ICON_SIZE: f64 = 14.0;
/// `renderer.ts`'s `ICON_GAP`.
pub const ICON_GAP: f64 = 4.0;
/// `renderer.ts`'s `DESC_GAP`.
pub const DESC_GAP: f64 = 16.0;
/// `styles.ts`' `.treeView-node-label { font-size: 16px }` and the description's size.
pub const LABEL_FONT_SIZE: f64 = 16.0;
/// `styles.ts`' `.treeView-node-label { fill: black }`.
pub const LABEL_COLOR: &str = "black";
/// `styles.ts`' `.treeView-node-line { stroke: black }`.
pub const LINE_COLOR: &str = "black";
/// `styles.ts`' `.treeView-node-icon { color: #546e7a }`, which the icons' `currentColor` takes.
pub const ICON_COLOR: &str = "#546e7a";
/// `styles.ts`' `.treeView-node-description { fill: #6a9955 }`.
pub const DESCRIPTION_COLOR: &str = "#6a9955";
/// `styles.ts`' `.treeView-highlight-bg { fill }`.
pub const HIGHLIGHT_BG: &str = "rgba(255, 193, 7, 0.15)";
/// `styles.ts`' `.treeView-highlight-bg { stroke }` and its `stroke-width: 1`.
pub const HIGHLIGHT_STROKE: &str = "#ffc107";
/// The highlight rectangle's `rx`.
pub const HIGHLIGHT_RADIUS: f64 = 3.0;

/// `treeViewIcons`' shared `viewBox` side.
const BUILTIN_ICON_VIEW_BOX: f64 = 24.0;
/// `treeViewIcons.folder`'s body.
const FOLDER_PATH: &str = "M10.59 4.59A2 2 0 0 0 9.17 4H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.17z";
/// `treeViewIcons.file`'s body, whose two subpaths need the even-odd rule.
const FILE_PATH: &str = "M6 2a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8.83a2 2 0 0 0-.59-1.42l-4.82-4.82A2 2 0 0 0 13.17 2H6Zm7.5 1.9l4.6 4.6h-3.6a1 1 0 0 1-1-1V3.9Z";
/// `unknownIcon`'s `viewBox` side.
const UNKNOWN_ICON_VIEW_BOX: f64 = 80.0;
/// `unknownIcon`'s box fill.
const UNKNOWN_ICON_FILL: &str = "#087ebf";
/// `unknownIcon`'s glyph.
const UNKNOWN_ICON_TEXT: &str = "?";
/// `unknownIcon`'s glyph position, its `<text transform="translate(21.16 64.67)">`.
const UNKNOWN_ICON_TEXT_POSITION: Point = Point::new(21.16, 64.67);
/// `unknownIcon`'s glyph `font-size`.
const UNKNOWN_ICON_TEXT_SIZE: f64 = 67.75;
/// `unknownIcon`'s glyph `font-family`.
const UNKNOWN_ICON_TEXT_FAMILY: &str = "ArialMT, Arial";
/// `unknownIcon`'s glyph fill.
const UNKNOWN_ICON_TEXT_FILL: &str = "#fff";
/// `treeViewIcons.prefix`.
const ICON_PACK: &str = "mermaid-treeview";

/// Renders a treeView body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(body: &str, config: &Config, measure: &dyn TextMeasure) -> Result<Scene, ParseError> {
    let ast = super::parse(body)?;
    Ok(draw_ast(&ast, config, measure))
}

/// Renders an already parsed treeView database.
pub fn draw_ast(ast: &Ast, config: &Config, measure: &dyn TextMeasure) -> Scene {
    let options = &config.tree_view;
    let measure_font = Font::new("monospace", LABEL_FONT_SIZE);

    // `drawTree` phase 1: one row per node in pre-order.
    let mut rows = Vec::new();
    let mut total_height = 0.0;
    let mut total_width = 0.0;
    collect_rows(
        &ast.root,
        0,
        options,
        &measure_font,
        measure,
        &mut total_height,
        &mut total_width,
        &mut rows,
    );

    // Phase 2: descriptions share one column, after the widest label.
    if rows.iter().any(|row| row.description.is_some()) {
        let max_label_right = rows
            .iter()
            .map(|row| row.label_right_edge)
            .fold(f64::NEG_INFINITY, f64::max);
        let desc_x = max_label_right + DESC_GAP;
        for row in rows.iter_mut() {
            let Some(description) = row.description.as_deref() else {
                continue;
            };
            let width = measure.width(description, &measure_font);
            row.description_position = Some(Point::new(desc_x, row.center_y));
            total_width = total_width.max(desc_x + width + options.padding_x);
        }
    }

    // Phase 3: highlight rectangles span the whole tree, widening it in turn.
    for row in rows.iter_mut() {
        if row.highlight {
            let width = total_width - row.bbox.x + 8.0;
            row.highlight_width = Some(width);
            total_width = total_width.max(row.bbox.x + width + 2.0);
        }
    }

    let view_box = Rect::new(
        -options.line_thickness / 2.0,
        0.0,
        total_width,
        total_height,
    );
    let mut scene = Scene::new(view_box);

    let mut group = Group::new().with_class(TREE_GROUP_CLASS);
    let mut cursor = 0;
    build_node(
        &ast.root,
        &rows,
        &mut cursor,
        &mut group,
        options,
        &measure_font,
        measure,
    );
    scene.push(group);
    scene
}

/// Renders a treeView body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, ParseError> {
    draw(body, config, &FallbackMeasure)
}

/// One row of the tree, the numbers `drawTree` keeps in a `RenderInfo` plus its node's box.
struct Row {
    /// `depth * (rowIndent + paddingX)`.
    indent: f64,
    /// `Node.BBox`.
    bbox: Rect,
    /// The label's `x`, after the indent, the padding and the icon offset.
    label_x: f64,
    /// `label.getBBox().width`.
    label_width: f64,
    /// `label.getBBox().height`, the row's ink height.
    label_height: f64,
    /// The label's `y`, the row's middle.
    center_y: f64,
    /// `labelRightEdge`.
    label_right_edge: f64,
    /// `node.description`, when it has one.
    description: Option<String>,
    /// Where phase 2 puts the description.
    description_position: Option<Point>,
    /// `node.cssClass` contains `highlight`.
    highlight: bool,
    /// The final `treeView-highlight-bg` width, set in phase 3.
    highlight_width: Option<f64>,
    /// The qualified icon reference `getNodeIcon` resolves.
    icon: Option<String>,
    /// The last child's row, for the vertical connector.
    last_child_row: Option<usize>,
}

/// `drawNode` plus the recursion: measures one node's row and then its children's.
#[allow(clippy::too_many_arguments)]
fn collect_rows(
    node: &Node,
    depth: usize,
    options: &TreeViewConfig,
    measure_font: &Font,
    measure: &dyn TextMeasure,
    total_height: &mut f64,
    total_width: &mut f64,
    rows: &mut Vec<Row>,
) {
    let index = rows.len();
    let indent = depth as f64 * (options.row_indent + options.padding_x);
    let icon = get_node_icon(node, options);
    let icon_offset = if icon.is_some() {
        ICON_SIZE + ICON_GAP
    } else {
        0.0
    };

    let label_height = if node.name.is_empty() {
        0.0
    } else {
        measure.line_height(measure_font)
    };
    let height = label_height + options.padding_y * 2.0;
    let label_width = measure.width(&node.name, measure_font);
    let label_x = indent + options.padding_x + icon_offset;
    let width = label_width + options.padding_x * 2.0 + icon_offset;
    let y = *total_height;

    *total_width = total_width.max(indent + width);
    *total_height = y + height;

    let row = Row {
        indent,
        bbox: Rect::new(indent, y, width, height),
        label_x,
        label_width,
        label_height,
        center_y: y + height / 2.0,
        label_right_edge: label_x + label_width,
        description: node.description.clone(),
        description_position: None,
        highlight: node
            .css_class
            .as_deref()
            .is_some_and(|class| class.split_whitespace().any(|part| part == "highlight")),
        highlight_width: None,
        icon,
        last_child_row: None,
    };
    rows.push(row);

    for child in &node.children {
        let child_row = rows.len();
        collect_rows(
            child,
            depth + 1,
            options,
            measure_font,
            measure,
            total_height,
            total_width,
            rows,
        );
        rows[index].last_child_row = Some(child_row);
    }
}

/// The SVG the scene draws, in DOM order: the node's group, its branch line, its children, and
/// its vertical connector.
#[allow(clippy::too_many_arguments)]
fn build_node(
    node: &Node,
    rows: &[Row],
    cursor: &mut usize,
    parent: &mut Group,
    options: &TreeViewConfig,
    measure_font: &Font,
    measure: &dyn TextMeasure,
) {
    let index = *cursor;
    *cursor += 1;
    let row = &rows[index];

    let mut group = Group::new();
    if let (true, Some(width)) = (row.highlight, row.highlight_width) {
        // `insert('rect', ':first-child')`: the background comes before the icon and label.
        group.push(RectShape::new(
            Rect::new(row.bbox.x, row.bbox.y + 1.0, width, row.bbox.height - 2.0),
            HIGHLIGHT_RADIUS,
            PathStyle {
                fill: Some(Color::new(HIGHLIGHT_BG)),
                stroke: Some(Color::new(HIGHLIGHT_STROKE)),
                stroke_width: 1.0,
                ..PathStyle::default()
            },
        ));
    }
    if let Some(icon) = &row.icon {
        group.push(icon_group(
            icon,
            Point::new(
                row.bbox.x + options.padding_x,
                row.bbox.y + options.padding_y,
            ),
            measure,
        ));
    }

    let mut label_font = measure_font.clone();
    if node.node_type == NodeType::Directory {
        label_font = label_font.bold();
    }
    group.push(label_run(
        &node.name,
        Point::new(row.label_x, row.center_y),
        label_font,
        row.label_width,
        row.label_height,
    ));

    if let (Some(description), Some(position)) =
        (row.description.as_deref(), row.description_position)
    {
        let font = measure_font.clone().italic();
        let width = measure.width(description, measure_font);
        let height = if description.is_empty() {
            0.0
        } else {
            measure.line_height(measure_font)
        };
        group.push(text_run(
            description,
            position,
            font,
            width,
            height,
            DESCRIPTION_COLOR,
            TextBaseline::Middle,
        ));
    }
    parent.push(group);

    parent.push(connector(
        row.indent - options.row_indent,
        row.center_y,
        row.indent,
        row.center_y,
        options.line_thickness,
    ));

    for child in &node.children {
        build_node(child, rows, cursor, parent, options, measure_font, measure);
    }

    if let Some(last_child_row) = row.last_child_row {
        let last = &rows[last_child_row];
        parent.push(connector(
            row.bbox.x + options.padding_x,
            row.bbox.y + row.bbox.height,
            row.bbox.x + options.padding_x,
            last.bbox.y + last.bbox.height / 2.0 + options.line_thickness / 2.0,
            options.line_thickness,
        ));
    }
}

/// One `<text>` element as a run: its measured `getBBox` width and ink height.
fn text_run(
    text: &str,
    position: Point,
    font: Font,
    width: f64,
    height: f64,
    fill: &str,
    baseline: TextBaseline,
) -> TextRun {
    let block = TextBlock::new(
        vec![TextLine::new(vec![TextSpan::plain(text)])],
        Size::new(width, height),
        height,
    );
    let mut run = TextRun::new(position, font, block);
    run.baseline = baseline;
    run.fill = Some(Color::new(fill));
    run
}

/// A label: `dominant-baseline="middle"`, black, measured at `label_width`.
fn label_run(
    name: &str,
    position: Point,
    font: Font,
    label_width: f64,
    ink_height: f64,
) -> TextRun {
    text_run(
        name,
        position,
        font,
        label_width,
        ink_height,
        LABEL_COLOR,
        TextBaseline::Middle,
    )
}

/// `positionLine`: a straight `<line>` with the configured thickness.
fn connector(x1: f64, y1: f64, x2: f64, y2: f64, line_thickness: f64) -> PathShape {
    let mut path = Path::new();
    path.move_to(Point::new(x1, y1)).line_to(Point::new(x2, y2));
    PathShape::new(path, PathStyle::stroked(LINE_COLOR, line_thickness))
}

/// The `<g class="treeView-node-icon">` holding a nested icon SVG at `origin`.
fn icon_group(icon: &str, origin: Point, measure: &dyn TextMeasure) -> Group {
    match icon {
        "mermaid-treeview:file" => {
            icon_path_group(origin, BUILTIN_ICON_VIEW_BOX, FILE_PATH, FillRule::EvenOdd)
        }
        "mermaid-treeview:folder" => icon_path_group(
            origin,
            BUILTIN_ICON_VIEW_BOX,
            FOLDER_PATH,
            FillRule::NonZero,
        ),
        _ => unknown_icon_group(origin, measure),
    }
}

/// A built-in icon: one path in the 24-unit pack, scaled to `ICON_SIZE`.
fn icon_path_group(origin: Point, view_box: f64, body: &str, fill_rule: FillRule) -> Group {
    let mut path = PathShape::new(parse_path_data(body), PathStyle::filled(ICON_COLOR));
    path.fill_rule = fill_rule;
    let mut group = Group::new().with_class(ICON_GROUP_CLASS).with_transform(
        Transform::scale(ICON_SIZE / view_box, ICON_SIZE / view_box)
            .then(Transform::translate(origin.x, origin.y)),
    );
    group.push(path);
    group
}

/// `unknownIcon`: the 80-unit blue box with a white `?`, scaled to `ICON_SIZE`.
fn unknown_icon_group(origin: Point, measure: &dyn TextMeasure) -> Group {
    let mut group = Group::new().with_class(ICON_GROUP_CLASS).with_transform(
        Transform::scale(
            ICON_SIZE / UNKNOWN_ICON_VIEW_BOX,
            ICON_SIZE / UNKNOWN_ICON_VIEW_BOX,
        )
        .then(Transform::translate(origin.x, origin.y)),
    );
    group.push(RectShape::new(
        Rect::new(0.0, 0.0, UNKNOWN_ICON_VIEW_BOX, UNKNOWN_ICON_VIEW_BOX),
        0.0,
        PathStyle::filled(UNKNOWN_ICON_FILL),
    ));
    let font = Font::new(UNKNOWN_ICON_TEXT_FAMILY, UNKNOWN_ICON_TEXT_SIZE);
    let width = measure.width(UNKNOWN_ICON_TEXT, &font);
    let height = measure.line_height(&font);
    group.push(text_run(
        UNKNOWN_ICON_TEXT,
        UNKNOWN_ICON_TEXT_POSITION,
        font,
        width,
        height,
        UNKNOWN_ICON_TEXT_FILL,
        TextBaseline::Alphabetic,
    ));
    group
}

/// `getNodeIcon`: an explicit `icon()` always wins (`none` hides), then `showIcons` maps a
/// file's name through the configured icon maps, then the built-in file/folder icon.
pub fn get_node_icon(node: &Node, config: &TreeViewConfig) -> Option<String> {
    if node.icon.as_deref() == Some("none") {
        return None;
    }
    if let Some(icon) = node.icon.as_deref() {
        return Some(qualify_icon(icon, &config.default_icon_pack));
    }
    if !config.show_icons {
        return None;
    }
    if node.node_type == NodeType::File
        && let Some(detected) = detect_icon(&node.name, config)
    {
        if detected == "none" {
            return None;
        }
        return Some(qualify_icon(&detected, &config.default_icon_pack));
    }
    Some(format!(
        "{ICON_PACK}:{}",
        match node.node_type {
            NodeType::Directory => "folder",
            NodeType::File => "file",
        }
    ))
}

/// `detectIcon`: an exact filename match wins over an extension match, and extensions match
/// case-insensitively with or without their leading dot.
fn detect_icon(name: &str, config: &TreeViewConfig) -> Option<String> {
    if let Some(icon) = map_string(&config.filename_icons, name)
        && !icon.is_empty()
    {
        return Some(icon);
    }
    let dot = name.rfind('.')?;
    if dot == 0 {
        return None;
    }
    let extension = name[dot..].to_lowercase();
    if let Some(icon) = map_string(&config.extension_icons, &extension) {
        return Some(icon);
    }
    map_string(&config.extension_icons, &extension[1..])
}

/// One entry of an icon map, when it holds a string.
fn map_string(map: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

/// `qualifyIcon`: a prefixed reference is used as-is, a built-in name or an empty default pack
/// resolves to the built-in pack, and any other name takes the configured pack.
fn qualify_icon(icon: &str, default_icon_pack: &str) -> String {
    if icon.contains(':') {
        return icon.to_string();
    }
    if matches!(icon, "file" | "folder") || default_icon_pack.is_empty() {
        return format!("{ICON_PACK}:{icon}");
    }
    format!("{default_icon_pack}:{icon}")
}

/// The tree's outer group class, as `draw` sets it.
pub const TREE_GROUP_CLASS: &str = "tree-view";

/// The icon group's class, so a painter can theme icons apart from labels.
pub const ICON_GROUP_CLASS: &str = "treeView-node-icon";

#[cfg(test)]
mod tests;
