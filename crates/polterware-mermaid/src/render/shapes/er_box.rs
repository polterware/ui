//! Classic port of upstream `rendering-elements/shapes/erBox.ts`.
//!
//! An ER entity with attributes draws a table: an outer rectangle, one row per attribute
//! split into the type, name, keys and comment columns, the alternating row fills, the
//! column and row divider lines, and a name row. An entity without attributes draws a plain
//! rectangle sized from its label instead, with the 100px `minEntityWidth` floor.
//!
//! Two upstream details are kept because they decide the measured node box the layout uses:
//! `PADDING` is `config.er.diagramPadding` (20) and `TEXT_PADDING` is
//! `config.er.entityPadding` (15), and the non-HTML label path (`htmlLabels: false`, the SVGs
//! these scenes paint) scales both by 1.25 for the table branch. The label-only branch runs
//! before that scaling and keeps 20/30. The theme's `rowEven`, `rowOdd` and `nodeBorder`
//! variables colour the table.
use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, get_node_classes, shape_style,
};
use crate::measure::{Font, TextOptions, shape_text};
use crate::scene::{
    Color, Group, Item, Path, PathShape, PathStyle, Point, Rect, RectShape, Size, SourceRef,
    TextRun, Transform,
};

/// `config.er.diagramPadding`'s default, used as `PADDING`.
const DIAGRAM_PADDING: f64 = 20.0;
/// `config.er.entityPadding`'s default, used as `TEXT_PADDING`.
const ENTITY_PADDING: f64 = 15.0;
/// `config.er.minEntityWidth`'s default.
const MIN_ENTITY_WIDTH: f64 = 100.0;
/// The non-HTML label path multiplies both paddings by this factor.
const SVG_PADDING_SCALE: f64 = 1.25;

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    if let Some(alias) = node.alias.as_deref()
        && !alias.is_empty()
    {
        node.label = alias.to_string();
    }
    if node.attributes.is_empty() {
        return draw_plain(node, ctx);
    }
    draw_table(node, ctx)
}

fn font(ctx: &ShapeContext<'_>) -> Font {
    let size = ctx
        .theme
        .color("fontSize")
        .and_then(|s| s.trim_end_matches("px").parse::<f64>().ok())
        .or_else(|| ctx.theme.number("fontSize"))
        .unwrap_or_else(|| Font::default().size);
    let family = ctx.theme.color("fontFamily").unwrap_or("Arial").to_string();
    Font {
        family,
        size,
        weight: crate::scene::NORMAL_WEIGHT,
        italic: false,
    }
}

fn shape_error(error: crate::measure::TextError) -> ShapeError {
    ShapeError {
        message: error.to_string(),
    }
}

fn measure_block(
    text: &str,
    font: &Font,
    width: f64,
    ctx: &ShapeContext<'_>,
) -> Result<Size, ShapeError> {
    let block = shape_text(
        text,
        &TextOptions {
            width,
            markdown: true,
            min_width: 0.0,
        },
        font,
        ctx.measure,
    )
    .map_err(shape_error)?;
    Ok(block.size)
}

fn label_block(
    text: &str,
    font: &Font,
    width: f64,
    ctx: &ShapeContext<'_>,
    source: &SourceRef,
) -> Result<(Group, Size), ShapeError> {
    let block = shape_text(
        text,
        &TextOptions {
            width,
            markdown: true,
            min_width: 0.0,
        },
        font,
        ctx.measure,
    )
    .map_err(shape_error)?;
    let size = block.size;
    let mut group = Group::new().with_class("label");
    group.source = Some(source.clone());
    group.push(TextRun::new(Point::ZERO, font.clone(), block));
    Ok((group, size))
}

fn draw_plain(mut node: Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let font = font(ctx);
    let source = SourceRef::node(node.id.clone());
    let mut group = Group::new()
        .with_class(get_node_classes(&node, None))
        .with_source(source.clone());

    let width = node.width.or(node.wrapping_width).unwrap_or(f64::INFINITY);
    let (mut label, text_size) = label_block(&node.label, &font, width, ctx, &source)?;

    let padding_x = DIAGRAM_PADDING;
    let padding_y = DIAGRAM_PADDING * 1.5;
    let padded_width = text_size.width + padding_x * 2.0;
    // Upstream replaces an explicit width with `minEntityWidth` when the padded label is narrow.
    let width = if padded_width < MIN_ENTITY_WIDTH {
        MIN_ENTITY_WIDTH
    } else {
        padded_width.max(node.width.unwrap_or(0.0))
    };
    let height = (text_size.height + padding_y * 2.0).max(node.height.unwrap_or(0.0));
    let bounds = Rect::centered(Point::ZERO, Size::new(width, height));
    node.bounds = bounds;

    let mut shape = Group::new().with_class("basic label-container");
    shape.push(RectShape::new(bounds, 0.0, shape_style()).with_source(source.clone()));
    group.push(shape);

    // `drawRect` centres the SVG text block on the rect; the scene's runs start at their origin.
    label.transform = Transform::translate(-text_size.width / 2.0, -text_size.height / 2.0);
    group.push(Item::Group(label));

    node.intersect = IntersectKind::Rect;
    Ok(RenderedNode { node, group })
}

struct Row {
    y_offset: f64,
    height: f64,
}

fn draw_table(mut node: Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let font = font(ctx);
    let source = SourceRef::node(node.id.clone());
    let mut group = Group::new()
        .with_class(get_node_classes(&node, None))
        .with_source(source.clone());

    let padding = DIAGRAM_PADDING * SVG_PADDING_SCALE;
    let text_padding = ENTITY_PADDING * SVG_PADDING_SCALE;

    let (mut name_label, name_size) = label_block(&node.label, &font, f64::INFINITY, ctx, &source)?;
    // Upstream grows the name box by one text padding before laying out the rows.
    let name_height = name_size.height + text_padding;

    let mut rows: Vec<Row> = Vec::new();
    let mut y_offset = 0.0;
    let mut max_type_width: f64 = 0.0;
    let mut max_name_width: f64 = 0.0;
    let mut max_keys_width: f64 = 0.0;
    let mut max_comment_width: f64 = 0.0;
    let mut shape_text_width: f64 = name_size.width;

    for attribute in &node.attributes {
        let keys = attribute.keys.join(",");
        let type_size = measure_block(&attribute.type_name, &font, f64::INFINITY, ctx)?;
        let name_size = measure_block(&attribute.name, &font, f64::INFINITY, ctx)?;
        let keys_size = measure_block(&keys, &font, f64::INFINITY, ctx)?;
        let comment_size = measure_block(&attribute.comment, &font, f64::INFINITY, ctx)?;

        max_type_width = max_type_width.max(type_size.width + padding);
        max_name_width = max_name_width.max(name_size.width + padding);
        max_keys_width = max_keys_width.max(keys_size.width + padding);
        max_comment_width = max_comment_width.max(comment_size.width + padding);
        shape_text_width = shape_text_width
            .max(type_size.width)
            .max(name_size.width)
            .max(keys_size.width)
            .max(comment_size.width);

        let row_height = type_size
            .height
            .max(name_size.height)
            .max(keys_size.height)
            .max(comment_size.height)
            + text_padding;
        rows.push(Row {
            y_offset,
            height: row_height,
        });
        y_offset += row_height;
    }

    let mut total_width_sections = 4.0;
    let has_keys = max_keys_width > padding;
    if !has_keys {
        max_keys_width = 0.0;
        total_width_sections -= 1.0;
    }
    let has_comment = max_comment_width > padding;
    if !has_comment {
        max_comment_width = 0.0;
        total_width_sections -= 1.0;
    }

    // Spread a wide name across the sections that exist, as upstream does.
    let difference = name_size.width + padding * 2.0
        - (max_type_width + max_name_width + max_keys_width + max_comment_width);
    if difference > 0.0 {
        max_type_width += difference / total_width_sections;
        max_name_width += difference / total_width_sections;
        if max_keys_width > 0.0 {
            max_keys_width += difference / total_width_sections;
        }
        if max_comment_width > 0.0 {
            max_comment_width += difference / total_width_sections;
        }
    }

    let max_width = max_type_width + max_name_width + max_keys_width + max_comment_width;
    let total_rows_height: f64 = rows.iter().map(|row| row.height).sum();
    let width = (shape_text_width + padding * 2.0)
        .max(node.width.unwrap_or(0.0))
        .max(max_width);
    let height = (total_rows_height + name_height).max(node.height.unwrap_or(0.0));
    let x = -width / 2.0;
    let y = -height / 2.0;
    let bounds = Rect::centered(Point::ZERO, Size::new(width, height));
    node.bounds = bounds;

    let mut outer = Group::new().with_class("basic label-container outer-path");
    outer.push(RectShape::new(bounds, 0.0, shape_style()).with_source(source.clone()));
    group.push(outer);

    let row_even = ctx.theme.color("rowEven").unwrap_or("#d6d6d6");
    let row_odd = ctx.theme.color("rowOdd").unwrap_or("#f2f2f2");
    let node_border = ctx.theme.color("nodeBorder").unwrap_or("#9370DB");
    let row_style = |fill: &str| PathStyle {
        fill: Some(Color::new(fill)),
        stroke: Some(Color::new(node_border)),
        ..Default::default()
    };

    for (index, row) in rows.iter().enumerate() {
        let content_row_index = index + 1;
        let is_even = content_row_index % 2 == 0 && row.y_offset != 0.0;
        let rect = Rect::new(x, name_height + y + row.y_offset, width, row.height);
        group.push(
            RectShape::new(
                rect,
                0.0,
                row_style(if is_even { row_even } else { row_odd }),
            )
            .with_source(source.clone()),
        );
    }

    // Divider lines: the name separator, the column separators, and one line per row.
    let mut dividers = Group::new().with_class("divider");
    let mut divider_line = |x1: f64, y1: f64, x2: f64, y2: f64| {
        let mut path = Path::new();
        path.move_to(Point::new(x1, y1)).line_to(Point::new(x2, y2));
        dividers.push(PathShape::new(path, shape_style()).with_source(source.clone()));
    };
    divider_line(x, name_height + y, x + width, name_height + y);
    divider_line(
        max_type_width + x,
        name_height + y,
        max_type_width + x,
        height + y,
    );
    if has_keys {
        let column = max_type_width + max_name_width + x;
        divider_line(column, name_height + y, column, height + y);
    }
    if has_comment {
        let column = max_type_width + max_name_width + max_keys_width + x;
        divider_line(column, name_height + y, column, height + y);
    }
    // `yOffsets` starts with 0, so the name separator is drawn a second time on the same line.
    divider_line(x, name_height + y, x + width, name_height + y);
    for row in &rows {
        let line_y = name_height + y + row.y_offset;
        divider_line(x, line_y, x + width, line_y);
    }
    group.push(dividers);

    // Text: the name centred on the header, then each attribute row at its column offset.
    name_label.transform = Transform::translate(-name_size.width / 2.0, y + text_padding / 2.0);
    group.push(Item::Group(name_label));

    for (row_index, attribute) in node.attributes.iter().enumerate() {
        let layout_row = &rows[row_index];
        let base_x = x + padding / 2.0;
        let base_y = y + name_height + text_padding / 2.0 + layout_row.y_offset;
        let columns: [(f64, &str, String); 4] = [
            (0.0, "attribute-type", attribute.type_name.clone()),
            (max_type_width, "attribute-name", attribute.name.clone()),
            (
                max_type_width + max_name_width,
                "attribute-keys",
                attribute.keys.join(","),
            ),
            (
                max_type_width + max_name_width + max_keys_width,
                "attribute-comment",
                attribute.comment.clone(),
            ),
        ];
        for (offset, class, text) in columns {
            let (mut label, _) = label_block(&text, &font, f64::INFINITY, ctx, &source)?;
            label.class = Some(format!("label {class}"));
            label.transform = Transform::translate(base_x + offset, base_y);
            group.push(Item::Group(label));
        }
    }

    node.intersect = IntersectKind::Rect;
    Ok(RenderedNode { node, group })
}
