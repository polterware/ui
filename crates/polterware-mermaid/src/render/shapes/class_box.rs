//! Classic port of upstream `shapes/classBox.ts` with its `textHelper` from
//! `diagrams/class/shapeUtil.ts`.
//!
//! The box stacks an annotation row (`«interface»`), the bold class name, the members and the
//! methods between two dividers. Upstream reads those rows from the `ClassNode` that
//! `classDb.ts` `getData()` spreads into the layout node; here they arrive on
//! [`Node::class_members`], [`Node::class_methods`], [`Node::annotations`] and [`Node::text`].
//!
//! Upstream takes the box padding from `config.class.padding` inside the shape. The layout
//! data carries that value as [`Node::class_padding`] and `hideEmptyMembersBox` as
//! [`Node::hide_empty_members_box`], because the shape context has no configuration.
//!
//! Labels are SVG text (`htmlLabels: false`), so `TEXT_PADDING` is 3 and the final
//! `- 4` non-HTML centering offset applies. HTML labels (`foreignObject`) are not
//! represented by the scene model.
use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, get_node_classes, shape_style,
};
use crate::measure::Font;
use crate::render::labels::{LabelOptions, create_label};
use crate::scene::{
    BOLD_WEIGHT, Color, Group, Item, NORMAL_WEIGHT, Path, PathShape, PathStyle, Point, Rect,
    RectShape, SourceRef, Transform,
};

/// `config.class.padding`'s default (the schema default of 5 is intentionally not used).
const PADDING: f64 = 12.0;
/// `TEXT_PADDING = !useHtmlLabels ? 3 : 0`.
const TEXT_PADDING: f64 = 3.0;
/// The non-HTML label centering offset `classBox.ts` applies to every text row.
const SVG_TEXT_OFFSET: f64 = 4.0;

/// A measured text row: the group a painter draws and the box it covers.
struct TextRow {
    group: Group,
    width: f64,
    height: f64,
}

/// A stack of text rows at increasing `yOffset`, as `textHelper` builds the members and
/// methods groups. `top` and `bottom` are the stack's local bbox extents.
struct RowStack {
    rows: Vec<TextRow>,
    top: f64,
    bottom: f64,
    width: f64,
}

impl RowStack {
    fn height(&self) -> f64 {
        self.bottom - self.top
    }
}

fn class_font(ctx: &ShapeContext<'_>) -> Font {
    let font_size = ctx
        .theme
        .color("fontSize")
        .and_then(|s| s.trim_end_matches("px").parse::<f64>().ok())
        .or_else(|| ctx.theme.number("fontSize"))
        .unwrap_or(Font::default().size);
    let font_family = ctx
        .theme
        .color("fontFamily")
        .map(|s| s.to_string())
        .unwrap_or_else(|| Font::default().family);
    let is_monospace = font_family.to_ascii_lowercase().contains("mono");
    Font {
        family: font_family,
        size: font_size,
        // Upstream styles the name `font-weight: bolder`; the monospace measure the harness
        // shares with `FallbackMeasure` is weight-independent, so the bold weight is applied
        // to the run after measuring.
        weight: if is_monospace {
            NORMAL_WEIGHT
        } else {
            BOLD_WEIGHT
        },
        italic: false,
    }
}

fn text_row(
    text: &str,
    font: &Font,
    y_offset: f64,
    ctx: &ShapeContext<'_>,
    source: &SourceRef,
) -> Result<TextRow, ShapeError> {
    // `addText`: SVG labels drop the leading `\` that marks an escaped visibility character.
    let text = text.strip_prefix('\\').unwrap_or(text);
    let mut group = create_label(
        &LabelOptions {
            text: text.to_string(),
            font: font.clone(),
            markdown: false,
            width: f64::INFINITY,
            centered: false,
            class: "label".into(),
            source: Some(source.clone()),
        },
        ctx.measure,
    )
    .map_err(|error| ShapeError {
        message: error.to_string(),
    })?;
    let (width, height) = match group.items.first() {
        Some(Item::Text(run)) => (run.size.width, run.size.height),
        _ => (0.0, 0.0),
    };
    // `addText` places the label at `-bbox.height / (2 * lines) + yOffset`; every class
    // row is one line.
    group.transform = Transform::translate(0.0, -height / 2.0 + y_offset);
    Ok(TextRow {
        group,
        width,
        height,
    })
}

fn bold_row(row: &mut TextRow) {
    if let Some(Item::Text(run)) = row.group.items.first_mut() {
        run.font.weight = BOLD_WEIGHT;
    }
}

fn stack(rows: Vec<TextRow>) -> RowStack {
    if rows.is_empty() {
        return RowStack {
            rows,
            top: 0.0,
            bottom: 0.0,
            width: 0.0,
        };
    }
    let first = rows[0].height;
    let mut offset = 0.0;
    let mut bottom = 0.0;
    let mut width: f64 = 0.0;
    for row in &rows {
        width = width.max(row.width);
        bottom = offset + row.height / 2.0;
        offset += row.height + TEXT_PADDING;
    }
    RowStack {
        rows,
        top: -first / 2.0,
        bottom,
        width,
    }
}

/// The box paint from `styles` declarations (`style` and `classDef`).
fn class_style(styles: &[String]) -> PathStyle {
    let mut style = shape_style();
    for declaration in styles.iter().flat_map(|style| style.split(';')) {
        let Some((key, value)) = declaration.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "fill" => style.fill = Some(Color::new(value)),
            "stroke" => style.stroke = Some(Color::new(value)),
            "stroke-width" => {
                if let Ok(width) = value.trim_end_matches("px").parse::<f64>() {
                    style.stroke_width = width;
                }
            }
            _ => {}
        }
    }
    style
}

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let padding = node.class_padding.unwrap_or(PADDING);
    let gap = padding;
    let source = SourceRef::node(node.id.clone());
    let members = node.class_members.clone();
    let methods = node.class_methods.clone();
    let empty = members.is_empty() && methods.is_empty();
    let render_extra_box = empty && !node.hide_empty_members_box;

    let mut group = Group::new()
        .with_class(get_node_classes(&node, None))
        .with_source(source.clone());

    let font = class_font(ctx);

    // `textHelper`: annotation row, bold name, members, methods.
    let annotation_text = node
        .annotations
        .first()
        .map(|annotation| format!("«{annotation}»"));
    let mut annotation_row = match &annotation_text {
        Some(text) => Some(text_row(text, &font, 0.0, ctx, &source)?),
        None => None,
    };
    let annotation_height_raw = annotation_row.as_ref().map_or(0.0, |row| row.height);

    let label_text = node.text.clone().unwrap_or_else(|| node.label.clone());
    let mut label_row = text_row(&label_text, &font, 0.0, ctx, &source)?;
    // Upstream styles the name `font-weight: bolder`; the monospace measure is
    // weight-independent, so the run is bolded after measuring.
    bold_row(&mut label_row);
    let label_height_raw = label_row.height;
    let label_width = label_row.width;

    let mut member_rows = Vec::with_capacity(members.len());
    let mut member_offset = 0.0;
    for member in &members {
        let mut row = text_row(&member.text, &font, member_offset, ctx, &source)?;
        if member.css_style.contains("font-style:italic")
            && let Some(Item::Text(run)) = row.group.items.first_mut()
        {
            run.font.italic = true;
        }
        member_offset += row.height + TEXT_PADDING;
        member_rows.push(row);
    }
    let members_stack = stack(member_rows);

    let mut method_rows = Vec::with_capacity(methods.len());
    let mut method_offset = 0.0;
    for method in &methods {
        let mut row = text_row(&method.text, &font, method_offset, ctx, &source)?;
        if method.css_style.contains("font-style:italic")
            && let Some(Item::Text(run)) = row.group.items.first_mut()
        {
            run.font.italic = true;
        }
        method_offset += row.height + TEXT_PADDING;
        method_rows.push(row);
    }
    let methods_stack = stack(method_rows);

    // `textHelper` heights: an empty members group measures `GAP / 2`.
    let members_height_raw = if members_stack.height() <= 0.0 {
        gap / 2.0
    } else {
        members_stack.height()
    };
    let members_group_y = annotation_height_raw + label_height_raw + gap * 2.0;
    let methods_group_y = annotation_height_raw
        + label_height_raw
        + if members_height_raw > 0.0 {
            members_height_raw + gap * 4.0
        } else {
            gap * 2.0
        };

    // The `shapeSvg` bbox `textHelper` returns.
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut include = |left: f64, right: f64, top: f64, bottom: f64| {
        if right > left && bottom > top {
            min_x = min_x.min(left);
            max_x = max_x.max(right);
            min_y = min_y.min(top);
            max_y = max_y.max(bottom);
        }
    };
    if let Some(row) = &annotation_row {
        include(
            -row.width / 2.0,
            row.width / 2.0,
            -row.height / 2.0,
            row.height / 2.0,
        );
    }
    include(
        -label_width / 2.0,
        label_width / 2.0,
        annotation_height_raw - label_height_raw / 2.0,
        annotation_height_raw + label_height_raw / 2.0,
    );
    if !members_stack.rows.is_empty() {
        include(
            0.0,
            members_stack.width,
            members_group_y + members_stack.top,
            members_group_y + members_stack.bottom,
        );
    }
    if !methods_stack.rows.is_empty() {
        include(
            0.0,
            methods_stack.width,
            methods_group_y + methods_stack.top,
            methods_group_y + methods_stack.bottom,
        );
    }
    let bbox_width = if min_x <= max_x { max_x - min_x } else { 0.0 };
    let bbox_height = if min_y <= max_y { max_y - min_y } else { 0.0 };

    // `classBox.ts` sizing.
    let w = node.width.unwrap_or(0.0).max(bbox_width);
    let mut h = node.height.unwrap_or(0.0).max(bbox_height);
    let node_height_greater = node.height.unwrap_or(0.0) > bbox_height;
    if empty {
        h += gap;
    } else if !members.is_empty() && methods.is_empty() {
        h += gap * 2.0;
    }

    let x = -w / 2.0;
    let y = -h / 2.0;
    let mut extra_height = if render_extra_box {
        padding * 2.0
    } else if empty {
        -padding
    } else {
        0.0
    };
    if node_height_greater {
        extra_height = padding * 2.0;
    }
    let rect_top_offset = if render_extra_box {
        padding
    } else if empty {
        -padding / 2.0
    } else {
        0.0
    };
    let rect = Rect::new(
        x - padding,
        y - padding - rect_top_offset,
        w + 2.0 * padding,
        h + 2.0 * padding + extra_height,
    );
    node.bounds = rect;

    let mut shape_group = Group::new().with_class("basic label-container outer-path");
    shape_group.push(
        RectShape::new(rect, 0.0, class_style(&node.class_styles)).with_source(source.clone()),
    );
    group.push(shape_group);

    // `classBox.ts` reads the group heights back and subtracts `PADDING / 2` for the empty
    // members box before placing dividers and text.
    let adjustment = if render_extra_box { padding / 2.0 } else { 0.0 };
    let annotation_height = if annotation_height_raw - adjustment == 0.0 {
        0.0
    } else {
        annotation_height_raw - adjustment
    };
    let label_height = if label_height_raw - adjustment == 0.0 {
        0.0
    } else {
        label_height_raw - adjustment
    };
    let members_height = if members_stack.height() - adjustment == 0.0 {
        0.0
    } else {
        members_stack.height() - adjustment
    };
    let members_height_for_methods = members_height.max(gap / 2.0);

    let shift = if render_extra_box {
        padding
    } else if empty {
        -padding / 2.0
    } else {
        0.0
    };

    let methods_area_placement = (annotation_height + label_height + y + padding - rect.y) / 2.0;
    let methods_text_y = if node_height_greater {
        methods_area_placement.max(
            annotation_height + label_height + members_height_for_methods + y + gap * 2.0 + padding,
        ) + gap * 2.0
    } else {
        annotation_height + label_height + members_height_for_methods + y + gap * 4.0 + padding
    };

    let place_row = |row: &TextRow, translate_x: f64, translate_y: f64, class: &str| {
        let mut parent = Group::new().with_class(class).with_source(source.clone());
        let mut child = row.group.clone();
        child.transform = Transform::translate(0.0, -row.height / 2.0);
        parent.push(Item::Group(child));
        parent.transform = Transform::translate(translate_x, translate_y - SVG_TEXT_OFFSET);
        parent
    };

    if let Some(row) = annotation_row.take() {
        group.push(place_row(
            &row,
            -row.width / 2.0,
            0.0 + y + padding - shift,
            "annotation-group text",
        ));
    }
    group.push(place_row(
        &label_row,
        -label_width / 2.0,
        annotation_height_raw + y + padding - shift,
        "label-group text",
    ));
    if !members_stack.rows.is_empty() {
        let mut parent = Group::new()
            .with_class("members-group text")
            .with_source(source.clone());
        parent.transform =
            Transform::translate(x, members_group_y + y + padding - shift - SVG_TEXT_OFFSET);
        for row in &members_stack.rows {
            let mut child = row.group.clone();
            child.transform = Transform::translate(0.0, -row.height / 2.0);
            parent.push(Item::Group(child));
        }
        group.push(parent);
    }
    if !methods_stack.rows.is_empty() {
        let mut parent = Group::new()
            .with_class("methods-group text")
            .with_source(source.clone());
        parent.transform = Transform::translate(x, methods_text_y - SVG_TEXT_OFFSET);
        for row in &methods_stack.rows {
            let mut child = row.group.clone();
            child.transform = Transform::translate(0.0, -row.height / 2.0);
            parent.push(Item::Group(child));
        }
        group.push(parent);
    }

    // `classBox.ts`: the first divider under the name, the second under the members.
    if !empty || render_extra_box {
        let first_line_y = annotation_height + label_height + y + padding;
        let second_line_y = if node_height_greater {
            methods_area_placement
                .max(annotation_height + label_height + members_height + y + gap * 2.0 + padding)
        } else {
            annotation_height + label_height + members_height + y + gap * 2.0 + padding
        };
        for line_y in [first_line_y, second_line_y] {
            let mut divider = Path::new();
            divider
                .move_to(Point::new(rect.x, line_y))
                .line_to(Point::new(rect.right(), line_y + 0.001));
            let mut divider_group = Group::new()
                .with_class("divider")
                .with_source(source.clone());
            divider_group.push(
                PathShape::new(
                    divider,
                    PathStyle::stroked(
                        class_style(&node.class_styles)
                            .stroke
                            .unwrap_or_else(|| Color::new("#9370DB")),
                        1.0,
                    ),
                )
                .with_source(source.clone()),
            );
            group.push(divider_group);
        }
    }

    node.intersect = IntersectKind::Rect;
    Ok(RenderedNode { node, group })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;
    use crate::render::intersect::Intersectable;
    use crate::scene::{Item, PathShape};
    use serde_json::Value;

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() <= 0.01, "{actual} != {expected}");
    }

    fn collect_paths<'a>(items: &'a [Item], out: &mut Vec<&'a PathShape>) {
        for item in items {
            match item {
                Item::Path(path) => out.push(path),
                Item::Group(group) => collect_paths(&group.items, out),
                _ => {}
            }
        }
    }

    #[test]
    fn matches_upstream_class_box_fixtures() {
        let fixture: Value =
            serde_json::from_str(include_str!("../testdata/shapes/class_box.json")).unwrap();
        let ctx = ShapeContext {
            measure: &FallbackMeasure,
            theme: &crate::theme::ThemeVariables::default(),
        };
        for case in fixture["cases"].as_array().unwrap() {
            let label = case["label"].as_str().unwrap();
            let mut node = Node::new("n1", label, "classBox");
            node.padding = 15.0;
            node.look = "classic".into();
            let rendered = draw(&node, &ctx).unwrap();
            close(
                rendered.node.bounds.width,
                case["bbox"]["width"].as_f64().unwrap(),
            );
            close(
                rendered.node.bounds.height,
                case["bbox"]["height"].as_f64().unwrap(),
            );
            for (index, query) in [(0, Point::new(1000.0, 0.0)), (1, Point::new(0.0, -1000.0))] {
                let hit = rendered.node.intersect(query);
                close(hit.x, case["intersects"][index]["x"].as_f64().unwrap());
                close(hit.y, case["intersects"][index]["y"].as_f64().unwrap());
            }

            let mut paths = Vec::new();
            collect_paths(&rendered.group.items, &mut paths);
            let dividers: Vec<_> = paths
                .into_iter()
                .filter(|path| path.path.commands.len() == 2)
                .collect();
            let expected: Vec<_> = case["elements"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|element| {
                    element["tag"] == "path"
                        && element["commands"]
                            .as_array()
                            .is_some_and(|commands| commands.len() == 4 && commands[1]["op"] == "C")
                })
                .collect();
            assert_eq!(dividers.len(), expected.len());
            for (divider, element) in dividers.iter().zip(expected) {
                let start = divider.path.commands[0].end_point().unwrap();
                let end = divider.path.commands[1].end_point().unwrap();
                close(start.x, element["commands"][0]["x"].as_f64().unwrap());
                close(start.y, element["commands"][0]["y"].as_f64().unwrap());
                close(end.x, element["commands"][1]["x"].as_f64().unwrap());
                close(end.y, element["commands"][1]["y"].as_f64().unwrap());
            }
        }
    }
}
