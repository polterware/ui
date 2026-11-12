//! Classic port of upstream `shapes/requirementBox.ts`.
//!
//! A requirement draws a box with the `<<Requirement type>>` stereotype, the bold name, and
//! the `ID:`, `Text:`, `Risk:` and `Verification:` body rows it has, divided from the name by
//! a rule. An element draws the same box with `<<Element>>`, its name, and its `Type:` and
//! `Doc Ref:` rows. `requirementDb.ts` `getData()` spreads the requirement or element fields
//! into the layout node, and `to_shape_node` carries them here as [`Node::requirement`] or
//! [`Node::element`], which is upstream's `'verifyMethod' in node` test.
//!
//! Measured geometry follows `addText`: each label is one SVG `<text>` whose bbox is its line
//! box (`1.15em` for the monospace measurement this crate uses), and the box keeps six pixels
//! of that label's height as spacing before the next row. The outer rectangle is the union of
//! the labels plus the 20px padding. Labels are centred inside the box; body rows start at
//! `x + padding / 2`, as SVG's `text-anchor: middle` at those translations reads.
//!
//! The theme's `requirementBackground` and `requirementBorderColor` paint the box and
//! `requirementBorderSize` its stroke, while `nodeTextColor`/`textColor` paint the labels,
//! as the `.reqBox` and `.label text` rules in `styles.js` do. Declarations from
//! `classDef`/`style` (`node.cssStyles`) override the fill, stroke, stroke width and label
//! colour. The `neo` divider (a 0.001-high polygon) and the `borderColorArray` palette stamp
//! are not painted: the classic divider line drawn here is the one both looks share, and the
//! palette is a paint-time CSS rule.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, get_node_classes};
use crate::measure::{Font, plain_text_block};
use crate::scene::{
    Color, Group, Item, Path, PathShape, PathStyle, Point, Rect, RectShape, Size, SourceRef, TextRun,
    Transform,
};

/// `const padding = 20` in `requirementBox`.
const PADDING: f64 = 20.0;
/// `const gap = 20` between the name and the first body row.
const GAP: f64 = 20.0;
/// `bbox.height += 6` in `addText`'s SVG branch.
const SVG_LABEL_EXTRA_HEIGHT: f64 = 6.0;

struct Label {
    group: Group,
    width: f64,
    /// The measured line box, without the extra spacing.
    height: f64,
    /// `addText`'s `yOffset`.
    y_offset: f64,
}

fn font(ctx: &ShapeContext<'_>, bold: bool) -> Font {
    let size = ctx
        .theme
        .color("fontSize")
        .and_then(|value| value.trim_end_matches("px").parse::<f64>().ok())
        .or_else(|| ctx.theme.number("fontSize"))
        .unwrap_or_else(|| Font::default().size);
    let family = ctx
        .theme
        .color("fontFamily")
        .unwrap_or("trebuchet ms")
        .to_string();
    // The name is bold upstream. The shared monospace measurement has a synthetic bold width
    // multiplier that a real monospace font does not have, so monospace names keep the normal
    // advance, as `requirementBox.ts` measurement in the harness does.
    let weight = if bold && !family.to_ascii_lowercase().contains("mono") {
        crate::scene::BOLD_WEIGHT
    } else {
        crate::scene::NORMAL_WEIGHT
    };
    Font {
        family,
        size,
        weight,
        italic: false,
    }
}

/// `addText`: measure one label and return its group, width, line height and row offset.
/// Empty text is skipped and contributes no height.
fn add_text(
    text: &str,
    font: Font,
    y_offset: f64,
    source: &SourceRef,
    ctx: &ShapeContext<'_>,
) -> Result<Option<Label>, ShapeError> {
    if text.is_empty() {
        return Ok(None);
    }
    let block = plain_text_block(text, &font, ctx.measure);
    let size = block.size;
    let mut group = Group::new().with_class("label").with_source(source.clone());
    group.push(TextRun::new(Point::ZERO, font, block));
    Ok(Some(Label {
        group,
        width: size.width,
        height: size.height,
        y_offset,
    }))
}

/// Parses the declarations in `node.cssStyles` that this shape paints.
struct StyleOverrides {
    fill: Option<Color>,
    stroke: Option<Color>,
    stroke_width: Option<f64>,
    label_color: Option<Color>,
}

impl StyleOverrides {
    fn from_node(node: &Node) -> Self {
        let mut overrides = Self {
            fill: None,
            stroke: None,
            stroke_width: None,
            label_color: None,
        };
        for declaration in &node.css_styles {
            let Some((key, value)) = declaration.split_once(':') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim().trim_end_matches("!important").trim();
            match key {
                "fill" => overrides.fill = Some(Color::new(value)),
                "stroke" => overrides.stroke = Some(Color::new(value)),
                "stroke-width" => {
                    overrides.stroke_width = value.trim_end_matches("px").parse::<f64>().ok();
                }
                "color" => overrides.label_color = Some(Color::new(value)),
                _ => {}
            }
        }
        overrides
    }
}

fn theme_color(
    ctx: &ShapeContext<'_>,
    name: &str,
    fallback: Option<&str>,
) -> Option<Color> {
    ctx.theme
        .color(name)
        .map(|value| Color::new(value.to_string()))
        .or_else(|| fallback.map(Color::new))
}

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let source = SourceRef::node(node.id.clone());
    let mut group = Group::new()
        .with_class(get_node_classes(&node, None))
        .with_source(source.clone());

    let requirement = node.requirement.clone();
    let element = node.element.clone();
    let overrides = StyleOverrides::from_node(&node);

    let body_color = overrides
        .label_color
        .clone()
        .or_else(|| theme_color(ctx, "nodeTextColor", Some("#333")))
        .or_else(|| theme_color(ctx, "textColor", Some("#333")));
    let normal = font(ctx, false);
    let name_font = font(ctx, true);

    let mut labels: Vec<Label> = Vec::new();
    let type_text = requirement
        .as_ref()
        .map(|requirement| format!("<<{}>>", requirement.type_name))
        .unwrap_or_else(|| "<<Element>>".to_string());
    if let Some(label) = add_text(&type_text, normal.clone(), 0.0, &source, ctx)? {
        labels.push(label);
    }
    let type_height = labels
        .last()
        .map(|label| label.height + SVG_LABEL_EXTRA_HEIGHT)
        .unwrap_or(0.0);

    let mut accumulative_height = type_height;
    if let Some(label) = add_text(
        &node.label,
        name_font,
        accumulative_height,
        &source,
        ctx,
    )? {
        labels.push(label);
    }
    let name_height = labels
        .last()
        .map(|label| label.height + SVG_LABEL_EXTRA_HEIGHT)
        .unwrap_or(0.0);
    accumulative_height += name_height + GAP;

    if let Some(requirement) = &requirement {
        let rows = [
            (!requirement.requirement_id.is_empty())
                .then(|| format!("ID: {}", requirement.requirement_id)),
            (!requirement.text.is_empty()).then(|| format!("Text: {}", requirement.text)),
            (!requirement.risk.is_empty()).then(|| format!("Risk: {}", requirement.risk)),
            (!requirement.verify_method.is_empty())
                .then(|| format!("Verification: {}", requirement.verify_method)),
        ];
        for row in rows.into_iter().flatten() {
            if let Some(label) = add_text(&row, normal.clone(), accumulative_height, &source, ctx)?
            {
                accumulative_height += label.height + SVG_LABEL_EXTRA_HEIGHT;
                labels.push(label);
            }
        }
    } else if let Some(element) = &element {
        let rows = [
            (!element.type_name.is_empty()).then(|| format!("Type: {}", element.type_name)),
            (!element.doc_ref.is_empty()).then(|| format!("Doc Ref: {}", element.doc_ref)),
        ];
        for row in rows.into_iter().flatten() {
            if let Some(label) = add_text(&row, normal.clone(), accumulative_height, &source, ctx)?
            {
                accumulative_height += label.height + SVG_LABEL_EXTRA_HEIGHT;
                labels.push(label);
            }
        }
    }

    // `shapeSvg.node().getBBox()`: the union of the label boxes, which start at their
    // `yOffset` and end one measured line later.
    let bbox_width = labels.iter().map(|label| label.width).fold(0.0, f64::max);
    let bbox_bottom = labels
        .iter()
        .map(|label| label.y_offset + label.height)
        .fold(0.0, f64::max);
    let total_width = bbox_width + PADDING;
    let total_height = bbox_bottom + PADDING;
    let bounds = Rect::centered(Point::ZERO, Size::new(total_width, total_height));
    node.bounds = bounds;

    let fill = overrides
        .fill
        .clone()
        .or_else(|| theme_color(ctx, "requirementBackground", Some("#ECECFF")));
    let stroke = overrides
        .stroke
        .clone()
        .or_else(|| theme_color(ctx, "requirementBorderColor", Some("#9370DB")));
    let stroke_width = overrides.stroke_width.unwrap_or_else(|| {
        ctx.theme
            .number("requirementBorderSize")
            .unwrap_or(1.0)
    });
    let mut style = PathStyle::default();
    style.fill = fill;
    style.stroke = stroke;
    style.stroke_width = stroke_width;
    group.push(RectShape::new(bounds, 0.0, style).with_source(source.clone()));

    // Re-translate the labels now that the rectangle is centred. Type and name keep the
    // centred x they were created with; body rows start at `x + padding / 2`.
    let x = bounds.x;
    let y = bounds.y;
    let has_body = accumulative_height > type_height + name_height + GAP;
    for (index, label) in labels.iter_mut().enumerate() {
        let translate_x = if index < 2 {
            -label.width / 2.0
        } else {
            x + PADDING / 2.0
        };
        let translate_y = label.y_offset - total_height / 2.0 + PADDING;
        label.group.transform = Transform::translate(translate_x, translate_y);
        if let Some(color) = &body_color
            && let Some(Item::Text(text)) = label.group.items.first_mut()
        {
            text.fill = Some(color.clone());
        }
        group.push(Item::Group(label.group.clone()));
    }

    // Insert the divider before the body text, as upstream inserts it in the shape group.
    if has_body {
        let line_y = y + type_height + name_height + GAP;
        let divider_color = theme_color(ctx, "nodeBorder", Some("#333"));
        let mut divider_style = PathStyle::default();
        divider_style.fill = None;
        divider_style.stroke = divider_color;
        divider_style.stroke_width = 1.0;
        let mut line = Path::new();
        line.move_to(Point::new(x, line_y));
        line.line_to(Point::new(x + total_width, line_y));
        let mut divider = Group::new()
            .with_class("divider")
            .with_source(source.clone());
        divider.push(PathShape::new(line, divider_style).with_source(source.clone()));
        group.push(Item::Group(divider));
    }

    node.intersect = IntersectKind::Rect;
    Ok(RenderedNode { node, group })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;
    use crate::render::pipeline::types::{ElementNodeData, RequirementNodeData};

    fn ctx() -> ShapeContext<'static> {
        let theme: &'static crate::theme::ThemeVariables =
            Box::leak(Box::new(crate::theme::ThemeVariables::default()));
        ShapeContext {
            measure: &FallbackMeasure,
            theme,
        }
    }

    fn node(label: &str, requirement: Option<RequirementNodeData>) -> Node {
        let mut node = Node::new("n1", label, "requirementBox");
        node.padding = 15.0;
        node.look = "classic".into();
        node.requirement = requirement;
        node
    }

    fn label_groups(items: &[Item]) -> Vec<&Group> {
        items
            .iter()
            .filter_map(|item| match item {
                Item::Group(group) if group.class.as_deref() == Some("label") => Some(group),
                _ => None,
            })
            .collect()
    }

    fn label_text(label: &Group) -> String {
        match label.items.first() {
            Some(Item::Text(text)) => text.lines.iter().map(|line| line.text()).collect(),
            _ => String::new(),
        }
    }

    #[test]
    fn requirement_box_draws_the_stereotype_name_and_body_rows() {
        let requirement = RequirementNodeData {
            type_name: "Requirement".to_string(),
            requirement_id: "x".to_string(),
            text: "the text".to_string(),
            risk: "High".to_string(),
            verify_method: "Test".to_string(),
        };
        let rendered = draw(&node("r", Some(requirement)), &ctx()).unwrap();
        let labels = label_groups(&rendered.group.items);
        let texts: Vec<String> = labels.iter().map(|label| label_text(label)).collect();
        assert_eq!(
            texts,
            [
                "<<Requirement>>",
                "r",
                "ID: x",
                "Text: the text",
                "Risk: High",
                "Verification: Test",
            ]
        );
        // 20px padding around the label union.
        assert!(rendered.node.bounds.width > 20.0);
        assert!(rendered.node.bounds.height > 20.0);
        assert!(rendered.group.items.iter().any(|item| matches!(
            item,
            Item::Group(group) if group.class.as_deref() == Some("divider")
        )));
    }

    #[test]
    fn element_box_draws_its_own_rows_and_no_divider_without_them() {
        let mut node = node("e", None);
        node.element = Some(ElementNodeData {
            type_name: "simulation".to_string(),
            doc_ref: "r1".to_string(),
        });
        let rendered = draw(&node, &ctx()).unwrap();
        let labels = label_groups(&rendered.group.items);
        let texts: Vec<String> = labels.iter().map(|label| label_text(label)).collect();
        assert_eq!(texts, ["<<Element>>", "e", "Type: simulation", "Doc Ref: r1"]);

        let rendered = draw(&node_with_no_rows(), &ctx()).unwrap();
        assert!(!rendered.group.items.iter().any(|item| matches!(
            item,
            Item::Group(group) if group.class.as_deref() == Some("divider")
        )));
    }

    fn node_with_no_rows() -> Node {
        let mut node = node("e", None);
        node.element = Some(ElementNodeData::default());
        node
    }

    #[test]
    fn style_declarations_override_the_theme_colours() {
        let mut node = node("r", Some(RequirementNodeData::default()));
        node.css_styles = vec![
            "fill:#f96".to_string(),
            "stroke:#333".to_string(),
            "stroke-width:2px".to_string(),
            "color:#fff".to_string(),
        ];
        let rendered = draw(&node, &ctx()).unwrap();
        let Item::Rect(rect) = rendered
            .group
            .items
            .iter()
            .find(|item| matches!(item, Item::Rect(_)))
            .expect("box rect")
        else {
            unreachable!()
        };
        assert_eq!(rect.style.fill, Some(Color::new("#f96")));
        assert_eq!(rect.style.stroke, Some(Color::new("#333")));
        assert_eq!(rect.style.stroke_width, 2.0);
        let labels = label_groups(&rendered.group.items);
        let Some(Item::Text(text)) = labels[0].items.first() else {
            panic!("label text")
        };
        assert_eq!(text.fill, Some(Color::new("#fff")));
    }
}
