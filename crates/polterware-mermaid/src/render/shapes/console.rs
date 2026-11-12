//! Classic port of upstream `shapes/console.ts` (`consoleWindow`): a rounded terminal window
//! with a `>_` glyph in the top band and the label centred in the body.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::measure::Font;
use crate::scene::{
    Color, Group, Item, Point, Rect, RectShape, Size, SourceRef, TextBlock, TextLine, TextRun,
    TextSpan, Transform,
};

/// Height of the prompt glyph band.
const GLYPH_BAND: f64 = 20.0;
/// Window corner radius.
const RADIUS: f64 = 12.0;
/// Minimum console width.
const MIN_WIDTH: f64 = 90.0;
/// Prompt glyph style, `font-family:monospace;font-weight:bold;font-size:14px`.
const GLYPH_SIZE: f64 = 14.0;

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (group, label, text_size) = prepare(&node, ctx)?;
    let accent = ctx
        .theme
        .color("nodeBorder")
        .or_else(|| ctx.theme.color("lineColor"))
        .map(str::to_string);
    let padding = node.padding;
    let w = (text_size.width + padding * 2.0)
        .max(node.width.unwrap_or(0.0))
        .max(MIN_WIDTH);
    let h = (text_size.height + padding * 2.0 + GLYPH_BAND).max(node.height.unwrap_or(0.0));
    let bounds = Rect::centered(Point::ZERO, Size::new(w, h));
    node.bounds = bounds;
    let top = -h / 2.0;

    let mut group = group;
    let mut shape = Group::new().with_class("basic label-container");
    // handDrawn: rough.js (later)
    shape.push(
        RectShape::new(bounds, RADIUS, shape_style()).with_source(SourceRef::node(node.id.clone())),
    );

    let glyph_font = Font::new("monospace", GLYPH_SIZE).bold();
    let glyph_size = Size::new(
        ctx.measure.width(">_", &glyph_font),
        ctx.measure.line_height(&glyph_font),
    );
    let block = TextBlock::new(
        vec![TextLine::new(vec![TextSpan::plain(">_")])],
        glyph_size,
        crate::measure::LINE_HEIGHT_EM * glyph_font.size,
    );
    let mut glyph = TextRun::new(Point::new(-w / 2.0 + 12.0, top + 16.0), glyph_font, block)
        .with_source(SourceRef::node(node.id.clone()));
    if let Some(color) = &accent {
        glyph = glyph.with_fill(Color::new(color.clone()));
    }
    let mut glyph_group = Group::new()
        .with_class("console-glyph")
        .with_source(SourceRef::node(node.id.clone()));
    glyph_group.push(glyph);
    shape.push(glyph_group);
    group.push(shape);

    let body_center_y = top + GLYPH_BAND + (h - GLYPH_BAND) / 2.0;
    let mut label = label;
    label.transform = Transform::translate(
        -text_size.width / 2.0,
        body_center_y - text_size.height / 2.0,
    );
    group.push(Item::Group(label));

    node.intersect = IntersectKind::Rect;
    Ok(RenderedNode { node, group })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;
    use crate::render::intersect::Intersectable;
    use crate::scene::{Item, RectShape};
    use serde_json::Value;

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() <= 0.01, "{actual} != {expected}");
    }

    fn collect_rects<'a>(items: &'a [Item], out: &mut Vec<&'a RectShape>) {
        for item in items {
            match item {
                Item::Rect(rect) => out.push(rect),
                Item::Group(group) => collect_rects(&group.items, out),
                _ => {}
            }
        }
    }

    fn glyph(items: &[Item]) -> Option<&TextRun> {
        for item in items {
            match item {
                Item::Text(text) if text.lines.iter().any(|line| line.text() == ">_") => {
                    return Some(text);
                }
                Item::Group(group) => {
                    if let Some(text) = glyph(&group.items) {
                        return Some(text);
                    }
                }
                _ => {}
            }
        }
        None
    }

    #[test]
    fn matches_upstream_console_fixtures() {
        let fixture: Value =
            serde_json::from_str(include_str!("../testdata/shapes/console.json")).unwrap();
        let ctx = ShapeContext {
            measure: &FallbackMeasure,
            theme: &crate::theme::ThemeVariables::default(),
        };
        for case in fixture["cases"].as_array().unwrap() {
            let label = case["label"].as_str().unwrap();
            let mut node = Node::new("n1", label, "console");
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
            let expected = case["elements"].as_array().unwrap()[0].clone();
            let mut rects = Vec::new();
            collect_rects(&rendered.group.items, &mut rects);
            let rect = rects.first().expect("rect");
            close(rect.rect.x, expected["x"].as_f64().unwrap());
            close(rect.rect.y, expected["y"].as_f64().unwrap());
            close(rect.rect.width, expected["width"].as_f64().unwrap());
            close(rect.rect.height, expected["height"].as_f64().unwrap());
            close(rect.rx, expected["rx"].as_f64().unwrap());

            let glyph = glyph(&rendered.group.items).expect("glyph");
            close(glyph.position.x, -rendered.node.bounds.width / 2.0 + 12.0);
            close(glyph.position.y, -rendered.node.bounds.height / 2.0 + 16.0);
        }
    }
}
