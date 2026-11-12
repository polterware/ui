//! Classic port of upstream `shapes/usecaseJsonTable.ts`.
//!
//! Look: the JSON data table of the web design system's use case diagram: a bordered title
//! cell above key/value rows, every cell padded 8 × 4 around its centred label.
//! Behavior: upstream `usecaseJsonTable.ts`; [`draw_with_rows`] takes the rows the upstream
//! node carries as `jsonRows`, because this crate's shared `Node` has no such field yet.
//!
//! The ARIA role, label and leaf cell classes (`usecase-json-border`, `usecase-json-cell`, ...)
//! belong to the DOM; the scene keeps the grid and row group classes. The compiled
//! `stroke-width` style that widens the outer border is not carried by [`Node`], so the
//! table always uses upstream's `DEFAULT_BORDER_WIDTH`. `handDrawn` is not rendered:
//! rough.js jitter stays a renderer TODO.

use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, get_node_classes, prepare,
    shape_style,
};
use crate::measure::Font;
use crate::render::labels::{LabelOptions, create_label};
use crate::scene::{Group, Item, Rect, RectShape, Size, SourceRef, Transform};

// handDrawn: rough.js (later)

const CELL_PADDING_X: f64 = 8.0;
const CELL_PADDING_Y: f64 = 4.0;
const DEFAULT_BORDER_WIDTH: f64 = 1.0;

/// One `jsonRows` entry: the key label and the value label of a table row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonRow {
    pub key: String,
    pub value: String,
}

impl JsonRow {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

// TODO(dedupe): sibling usecase shapes measure extra labels the same way; this mirrors
// `insertLabel` for a centered SVG label.
fn measure_label(
    text: &str,
    class: &str,
    node_id: &str,
    ctx: &ShapeContext<'_>,
) -> Result<(Group, Size), ShapeError> {
    let label = create_label(
        &LabelOptions {
            text: text.to_string(),
            font: Font::default(),
            markdown: false,
            width: f64::INFINITY,
            centered: true,
            class: class.to_string(),
            source: Some(SourceRef::node(node_id)),
        },
        ctx.measure,
    )
    .map_err(|error| ShapeError {
        message: error.to_string(),
    })?;
    let size = match label.items.first() {
        Some(Item::Text(text)) => text.size,
        _ => Size::ZERO,
    };
    Ok((label, size))
}

/// `positionLabel`: centre the measured label box on `(center_x, center_y)`.
fn position_label(label: &mut Group, size: Size, center_x: f64, center_y: f64) {
    label.transform =
        Transform::translate(center_x - size.width / 2.0, center_y - size.height / 2.0);
}

fn cell_rect(rect: Rect, node_id: &str) -> RectShape {
    let mut shape = RectShape::new(rect, 0.0, shape_style());
    shape.source = Some(SourceRef::node(node_id));
    shape
}

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    draw_with_rows(node, ctx, &[])
}

/// Draws the table with its key/value rows.
pub fn draw_with_rows(
    node: &Node,
    ctx: &ShapeContext<'_>,
    rows: &[JsonRow],
) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, mut title_label, title_box) = prepare(&node, ctx)?;
    group.class = Some(get_node_classes(&node, Some("usecase-json-table")));
    title_label.class = Some("label usecase-json-title".to_string());

    let mut measured_rows = Vec::with_capacity(rows.len());
    for row in rows {
        let (key_label, key_size) =
            measure_label(&row.key, "label usecase-json-key", &node.id, ctx)?;
        let (value_label, value_size) =
            measure_label(&row.value, "label usecase-json-value", &node.id, ctx)?;
        measured_rows.push((key_label, key_size, value_label, value_size));
    }

    let key_width = measured_rows
        .iter()
        .map(|(_, key, _, _)| key.width)
        .fold(0.0_f64, f64::max)
        + CELL_PADDING_X * 2.0;
    let measured_value_width = measured_rows
        .iter()
        .map(|(_, _, _, value)| value.width)
        .fold(0.0_f64, f64::max)
        + CELL_PADDING_X * 2.0;
    let title_width = title_box.width + CELL_PADDING_X * 2.0;
    let inner_width = title_width.max(key_width + measured_value_width);
    let value_width =
        measured_value_width + (inner_width - key_width - measured_value_width).max(0.0);
    let title_height = title_box.height + CELL_PADDING_Y * 2.0;
    let row_heights: Vec<f64> = measured_rows
        .iter()
        .map(|(_, key, _, value)| key.height.max(value.height) + CELL_PADDING_Y * 2.0)
        .collect();
    let inner_height = title_height + row_heights.iter().sum::<f64>();
    let border_width = DEFAULT_BORDER_WIDTH;
    let total_width = inner_width + border_width * 2.0;
    let total_height = inner_height + border_width * 2.0;
    let left = -inner_width / 2.0;
    let top = -inner_height / 2.0;

    let mut table = Group::new().with_class("usecase-json-table-grid");
    table.source = Some(SourceRef::node(node.id.clone()));

    // The outer border is inserted first, so it paints under the grid.
    group.push(cell_rect(
        Rect::new(
            -total_width / 2.0,
            -total_height / 2.0,
            total_width,
            total_height,
        ),
        &node.id,
    ));

    table.push(cell_rect(
        Rect::new(left, top, inner_width, title_height),
        &node.id,
    ));
    position_label(&mut title_label, title_box, 0.0, top + title_height / 2.0);
    table.push(Item::Group(title_label));

    let mut row_top = top + title_height;
    for (index, (key_label, key_size, value_label, value_size)) in
        measured_rows.into_iter().enumerate()
    {
        let row_height = row_heights[index];
        let mut row_group = Group::new()
            .with_class("usecase-json-row")
            .with_transform(Transform::translate(0.0, row_top));
        row_group.source = Some(SourceRef::node(node.id.clone()));
        // `insertCell` inserts each cell as the row's first child, so the value cell lands
        // before the key cell; the labels follow both.
        row_group.push(cell_rect(
            Rect::new(left + key_width, 0.0, value_width, row_height),
            &node.id,
        ));
        row_group.push(cell_rect(
            Rect::new(left, 0.0, key_width, row_height),
            &node.id,
        ));

        let mut key_label = key_label;
        let mut value_label = value_label;
        position_label(
            &mut key_label,
            key_size,
            left + key_width / 2.0,
            row_height / 2.0,
        );
        position_label(
            &mut value_label,
            value_size,
            left + key_width + value_width / 2.0,
            row_height / 2.0,
        );
        row_group.push(Item::Group(key_label));
        row_group.push(Item::Group(value_label));
        table.push(Item::Group(row_group));

        row_top += row_height;
    }

    group.push(Item::Group(table));

    node.bounds = Rect::new(
        -total_width / 2.0,
        -total_height / 2.0,
        total_width,
        total_height,
    );
    node.intersect = IntersectKind::Rect;
    Ok(RenderedNode { node, group })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;
    use crate::theme::ThemeVariables;

    #[test]
    fn empty_table_is_the_title_cell_plus_border() {
        let node = Node::new("t", "Title", "usecaseJsonTable");
        let rendered = draw(
            &node,
            &ShapeContext {
                measure: &FallbackMeasure,
                theme: &ThemeVariables::default(),
            },
        )
        .unwrap();
        let table = match &rendered.group.items[1] {
            Item::Group(table) => table,
            _ => panic!("the grid follows the border"),
        };
        assert_eq!(table.class.as_deref(), Some("usecase-json-table-grid"));
        assert_eq!(table.items.len(), 2);
    }

    #[test]
    fn rows_add_one_group_each_and_widen_the_table() {
        let node = Node::new("t", "Title", "usecaseJsonTable");
        let rows = [JsonRow::new("name", "value"), JsonRow::new("a", "b")];
        let empty = draw(
            &node,
            &ShapeContext {
                measure: &FallbackMeasure,
                theme: &ThemeVariables::default(),
            },
        )
        .unwrap();
        let table = draw_with_rows(
            &node,
            &ShapeContext {
                measure: &FallbackMeasure,
                theme: &ThemeVariables::default(),
            },
            &rows,
        )
        .unwrap();
        assert!(table.node.bounds.height > empty.node.bounds.height);
        let grid = match &table.group.items[1] {
            Item::Group(grid) => grid,
            _ => panic!("the grid follows the border"),
        };
        let groups = grid
            .items
            .iter()
            .filter(|item| matches!(item, Item::Group(_)))
            .count();
        assert_eq!(groups, rows.len() + 1);
    }

    #[test]
    fn matches_upstream_fixtures() {
        use crate::render::shapes::usecase_ellipse::difftest;
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../testdata/shapes/usecase_json_table.json"))
                .expect("fixture parses");
        let theme = ThemeVariables::default();
        for case in fixture["cases"].as_array().expect("cases") {
            let measure = difftest::FixedMeasure::from_case(case);
            let node = difftest::node_for(case, "usecaseJsonTable");
            let rows: Vec<JsonRow> = case["rows"]
                .as_array()
                .map(|rows| {
                    rows.iter()
                        .map(|row| {
                            JsonRow::new(
                                row[0].as_str().expect("key"),
                                row[1].as_str().expect("value"),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            let rendered = draw_with_rows(
                &node,
                &ShapeContext {
                    measure: &measure,
                    theme: &theme,
                },
                &rows,
            )
            .expect("shape renders");
            difftest::assert_case(&rendered, case);
        }
    }
}
