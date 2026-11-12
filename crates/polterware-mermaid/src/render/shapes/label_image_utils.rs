//! Upstream `shapes/labelImageUtils.ts`.
//!
//! The upstream helper is DOM-only: `configureLabelImages` waits for every `<img>` in a label
//! to load (or fail), then applies the flex styles that make the image measurable, and
//! `hasTextBesidesImages` decides whether the label has text besides those images. This port
//! keeps the decisions and leaves the DOM work to the caller: it models the label tree, the
//! font-size parsing, and the exact styles upstream writes once an image is complete.
//!
//! No glyph or image bytes are read here; the crate stays free of a DOM.

/// Upstream's `enlargingFactor` for an image-only label: five times the body font size.
pub const LABEL_IMAGE_ENLARGING_FACTOR: f64 = 5.0;

/// Upstream `defaultConfig.fontSize`, used when the body font size cannot be parsed.
pub const DEFAULT_FONT_SIZE: f64 = 16.0;

/// Describes whether a label tree contains non-whitespace text apart from image elements.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LabelContent<'a> {
    Text(&'a str),
    Image,
    Other(Vec<LabelContent<'a>>),
}

/// `hasTextBesidesImages`: an image is never text; any non-whitespace text node is.
pub fn has_text_besides_images(content: &LabelContent<'_>) -> bool {
    match content {
        LabelContent::Text(text) => !text.trim().is_empty(),
        LabelContent::Image => false,
        LabelContent::Other(children) => children.iter().any(has_text_besides_images),
    }
}

/// `parseFontSize`: JavaScript `parseInt`, so a unit suffix is dropped and a fractional part is
/// truncated (`"16.9px"` is 16). A value with no leading integer is `None`, as upstream's
/// destructured `undefined` is.
pub fn parse_font_size(value: &str) -> Option<f64> {
    let trimmed = value.trim_start();
    let mut chars = trimmed.chars().peekable();
    let mut number = String::new();
    if matches!(chars.peek(), Some('+') | Some('-')) {
        number.push(chars.next()?);
    }
    while let Some(ch) = chars.peek() {
        if ch.is_ascii_digit() {
            number.push(*ch);
            chars.next();
        } else {
            break;
        }
    }
    if number.is_empty() || number == "+" || number == "-" {
        return None;
    }
    number.parse().ok()
}

/// The body font size `configureLabelImages` measures with: the configured `fontSize` when it
/// is set, otherwise the computed body font size, falling back to [`DEFAULT_FONT_SIZE`] when
/// neither parses. Upstream reads `getConfig().fontSize` first and only then the body style.
pub fn resolve_font_size(
    config_font_size: Option<&str>,
    body_font_size: Option<&str>,
    default_font_size: f64,
) -> f64 {
    let source = config_font_size
        .filter(|value| !value.is_empty())
        .or(body_font_size);
    source
        .and_then(parse_font_size)
        .unwrap_or(default_font_size)
}

/// Image sizing selected by `configureLabelImages` after load/error completion: an image-only
/// label gets a fixed five-font-size width, a label with text lets the image fill its container.
pub fn image_width(no_text: bool, font_size: f64) -> ImageWidth {
    if no_text {
        ImageWidth::Fixed(LABEL_IMAGE_ENLARGING_FACTOR * font_size)
    } else {
        ImageWidth::Container
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImageWidth {
    Fixed(f64),
    Container,
}

/// The image styles written by upstream `configureLabelImages` once an image loads or fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageLayout {
    pub display: &'static str,
    pub flex_direction: &'static str,
    pub width_percent: Option<u8>,
    pub min_width_px: Option<u32>,
    pub max_width_px: Option<u32>,
}

/// Resolves the CSS decisions made after image completion. A DOM adapter applies this value.
pub fn configured_image_layout(no_text: bool, font_size: f64) -> ImageLayout {
    match image_width(no_text, font_size) {
        ImageWidth::Fixed(width) => ImageLayout {
            display: "flex",
            flex_direction: "column",
            width_percent: None,
            min_width_px: Some(width.round() as u32),
            max_width_px: Some(width.round() as u32),
        },
        ImageWidth::Container => ImageLayout {
            display: "flex",
            flex_direction: "column",
            width_percent: Some(100),
            min_width_px: None,
            max_width_px: None,
        },
    }
}

use super::{Node, RenderedNode, ShapeContext, ShapeError};

// Compatibility entry for the current registry; this upstream module is a helper, not a shape.
pub fn draw(node: &Node, _ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    Err(ShapeError {
        message: format!("{} is a rendering helper, not a shape", node.shape),
    })
}

/// Differential-test support shared by the shape modules that own fixtures.
#[cfg(test)]
pub(crate) mod fixtures {
    use crate::measure::{Font, TextMeasure};
    use crate::render::intersect::Intersectable;
    use crate::render::shapes::{Node, RenderedNode, ShapeContext, ShapeError};
    use crate::scene::{EllipseShape, Group, Item, Path, PathCommand, Point, RectShape};
    use crate::theme::ThemeVariables;
    use serde::Deserialize;

    /// The tolerance every numeric comparison uses, in scene units.
    pub const TOLERANCE: f64 = 0.01;

    #[derive(Deserialize)]
    pub struct Fixture {
        pub cases: Vec<Case>,
    }

    #[derive(Deserialize)]
    pub struct Case {
        pub label: String,
        pub label_size: LabelSize,
        pub padding: f64,
        pub look: String,
        pub node: NodeSize,
        pub drawables: Vec<FixtureDrawable>,
        pub label_transform: [f64; 2],
        pub intersect: Vec<Intersection>,
    }

    #[derive(Deserialize)]
    pub struct LabelSize {
        pub width: f64,
        pub height: f64,
    }

    #[derive(Deserialize)]
    pub struct NodeSize {
        pub width: f64,
        pub height: f64,
    }

    #[derive(Deserialize)]
    pub struct Intersection {
        pub point: [f64; 2],
        pub at: [f64; 2],
    }

    #[derive(Deserialize)]
    #[serde(tag = "kind", rename_all = "camelCase")]
    pub enum FixtureDrawable {
        Path {
            d: String,
            translate: [f64; 2],
            chain: String,
        },
        Rect {
            x: f64,
            y: f64,
            width: f64,
            height: f64,
            rx: f64,
            ry: f64,
            translate: [f64; 2],
            chain: String,
        },
        Circle {
            cx: f64,
            cy: f64,
            r: f64,
            translate: [f64; 2],
            chain: String,
        },
    }

    /// A text measurer that returns the fixture's fixed label box.
    pub struct FixedMeasure {
        pub width: f64,
        pub height: f64,
    }

    impl TextMeasure for FixedMeasure {
        fn width(&self, _text: &str, _font: &Font) -> f64 {
            self.width
        }

        fn line_height(&self, _font: &Font) -> f64 {
            self.height
        }
    }

    #[derive(Debug)]
    enum Actual {
        Path {
            path: Path,
            translate: Point,
            chain: String,
        },
        Rect {
            rect: RectShape,
            translate: Point,
            chain: String,
        },
        Ellipse {
            ellipse: EllipseShape,
            translate: Point,
            chain: String,
        },
    }

    /// Draws one fixture case with `draw` and compares bounds, label position, drawables, and
    /// two attachment points against the upstream recording.
    pub fn assert_case<F>(case: &Case, draw: F)
    where
        F: Fn(&Node, &ShapeContext<'_>) -> Result<RenderedNode, ShapeError>,
    {
        let measure = FixedMeasure {
            width: case.label_size.width,
            height: case.label_size.height,
        };
        let theme = ThemeVariables::default();
        let mut node = Node::new("fixture", case.label.clone(), "fixture");
        node.padding = case.padding;
        node.look = case.look.clone();
        let ctx = ShapeContext {
            measure: &measure,
            theme: &theme,
        };
        let rendered = draw(&node, &ctx).expect("shape renders");
        assert_close(rendered.node.bounds.width, case.node.width, "node width");
        assert_close(rendered.node.bounds.height, case.node.height, "node height");

        let label = find_label(&rendered.group).expect("label group");
        assert_close(
            label.transform.e,
            case.label_transform[0],
            "label transform x",
        );
        assert_close(
            label.transform.f,
            case.label_transform[1],
            "label transform y",
        );

        let mut actual = Vec::new();
        flatten(&rendered.group, Point::ZERO, "", &mut actual);
        assert_eq!(
            actual.len(),
            case.drawables.len(),
            "drawable count for case {:?}",
            case.label
        );
        for (actual, expected) in actual.iter().zip(&case.drawables) {
            compare_drawable(actual, expected);
        }

        for intersection in &case.intersect {
            let hit = rendered
                .node
                .intersect(Point::new(intersection.point[0], intersection.point[1]));
            assert_close(hit.x, intersection.at[0], "intersection x");
            assert_close(hit.y, intersection.at[1], "intersection y");
        }
    }

    /// Asserts two coordinates agree within [`TOLERANCE`].
    pub fn assert_close(actual: f64, expected: f64, what: &str) {
        assert!(
            (actual - expected).abs() <= TOLERANCE,
            "{what}: expected {expected}, got {actual}"
        );
    }

    fn find_label(group: &Group) -> Option<&Group> {
        for item in &group.items {
            if let Item::Group(child) = item {
                let is_label = child
                    .class
                    .as_deref()
                    .is_some_and(|class| class.split_whitespace().any(|part| part == "label"));
                if is_label {
                    return Some(child);
                }
                if let Some(found) = find_label(child) {
                    return Some(found);
                }
            }
        }
        None
    }

    fn flatten(group: &Group, offset: Point, parent_chain: &str, out: &mut Vec<Actual>) {
        let chain = match group.class.as_deref() {
            Some(class) if !class.trim().is_empty() => {
                if parent_chain.is_empty() {
                    class.trim().to_string()
                } else {
                    format!("{parent_chain} {}", class.trim())
                }
            }
            _ => parent_chain.to_string(),
        };
        for item in &group.items {
            match item {
                Item::Group(child) => {
                    let is_label = child
                        .class
                        .as_deref()
                        .is_some_and(|class| class.split_whitespace().any(|part| part == "label"));
                    if is_label {
                        continue;
                    }
                    flatten(
                        child,
                        Point::new(offset.x + child.transform.e, offset.y + child.transform.f),
                        &chain,
                        out,
                    );
                }
                Item::Path(path) => out.push(Actual::Path {
                    path: path.path.clone(),
                    translate: offset,
                    chain: chain.clone(),
                }),
                Item::Rect(rect) => out.push(Actual::Rect {
                    rect: rect.clone(),
                    translate: offset,
                    chain: chain.clone(),
                }),
                Item::Ellipse(ellipse) => out.push(Actual::Ellipse {
                    ellipse: ellipse.clone(),
                    translate: offset,
                    chain: chain.clone(),
                }),
                Item::Text(_) | Item::Image(_) => {}
            }
        }
    }

    fn compare_drawable(actual: &Actual, expected: &FixtureDrawable) {
        match (actual, expected) {
            (
                Actual::Path {
                    path,
                    translate,
                    chain,
                },
                FixtureDrawable::Path {
                    d,
                    translate: expected_translate,
                    chain: expected_chain,
                },
            ) => {
                assert_eq!(chain, expected_chain, "path class chain");
                let expected_commands = parse_path(d);
                let actual_commands = path
                    .commands
                    .iter()
                    .map(|command| map_command(*command))
                    .collect::<Vec<_>>();
                assert_eq!(
                    actual_commands.len(),
                    expected_commands.len(),
                    "path command count"
                );
                assert_close(translate.x, expected_translate[0], "path translate x");
                assert_close(translate.y, expected_translate[1], "path translate y");
                for (actual, expected) in actual_commands.iter().zip(&expected_commands) {
                    compare_command(actual, expected);
                }
            }
            (
                Actual::Rect {
                    rect,
                    translate,
                    chain,
                },
                FixtureDrawable::Rect {
                    x,
                    y,
                    width,
                    height,
                    rx,
                    ry,
                    translate: expected_translate,
                    chain: expected_chain,
                },
            ) => {
                assert_eq!(chain, expected_chain, "rect class chain");
                assert_close(
                    rect.rect.x,
                    x + expected_translate[0] - translate.x,
                    "rect x",
                );
                assert_close(
                    rect.rect.y,
                    y + expected_translate[1] - translate.y,
                    "rect y",
                );
                assert_close(rect.rect.width, *width, "rect width");
                assert_close(rect.rect.height, *height, "rect height");
                assert_close(rect.rx, *rx, "rect rx");
                assert_close(rect.ry, *ry, "rect ry");
            }
            (
                Actual::Ellipse {
                    ellipse,
                    translate,
                    chain,
                },
                FixtureDrawable::Circle {
                    cx,
                    cy,
                    r,
                    translate: expected_translate,
                    chain: expected_chain,
                },
            ) => {
                assert_eq!(chain, expected_chain, "circle class chain");
                assert_close(translate.x, expected_translate[0], "circle translate x");
                assert_close(translate.y, expected_translate[1], "circle translate y");
                assert_close(ellipse.center.x, *cx, "circle cx");
                assert_close(ellipse.center.y, *cy, "circle cy");
                assert_close(ellipse.radius_x, *r, "circle r");
                assert_close(ellipse.radius_y, *r, "circle r");
            }
            _ => panic!("drawable kind mismatch"),
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Command {
        Move(f64, f64),
        Line(f64, f64),
        Arc {
            rx: f64,
            ry: f64,
            rotation: f64,
            large: bool,
            sweep: bool,
            x: f64,
            y: f64,
        },
        Close,
    }

    fn map_command(command: PathCommand) -> Command {
        match command {
            PathCommand::MoveTo { x, y } => Command::Move(x, y),
            PathCommand::LineTo { x, y } => Command::Line(x, y),
            PathCommand::ArcTo {
                rx,
                ry,
                rotation,
                large_arc,
                sweep,
                x,
                y,
            } => Command::Arc {
                rx,
                ry,
                rotation,
                large: large_arc,
                sweep,
                x,
                y,
            },
            PathCommand::Close => Command::Close,
            other => panic!("unexpected path command in fixture comparison: {other:?}"),
        }
    }

    fn compare_command(actual: &Command, expected: &Command) {
        match (actual, expected) {
            (Command::Move(ax, ay), Command::Move(ex, ey)) => {
                assert_close(*ax, *ex, "move x");
                assert_close(*ay, *ey, "move y");
            }
            (Command::Line(ax, ay), Command::Line(ex, ey)) => {
                assert_close(*ax, *ex, "line x");
                assert_close(*ay, *ey, "line y");
            }
            (
                Command::Arc {
                    rx,
                    ry,
                    rotation,
                    large,
                    sweep,
                    x,
                    y,
                },
                Command::Arc {
                    rx: erx,
                    ry: ery,
                    rotation: erotation,
                    large: elarge,
                    sweep: esweep,
                    x: ex,
                    y: ey,
                },
            ) => {
                assert_close(*rx, *erx, "arc rx");
                assert_close(*ry, *ery, "arc ry");
                assert_close(*rotation, *erotation, "arc rotation");
                assert_eq!(large, elarge, "arc large flag");
                assert_eq!(sweep, esweep, "arc sweep flag");
                assert_close(*x, *ex, "arc x");
                assert_close(*y, *ey, "arc y");
            }
            (Command::Close, Command::Close) => {}
            _ => panic!("path command mismatch: {actual:?} vs {expected:?}"),
        }
    }

    enum Token {
        Command(char),
        Number(f64),
    }

    fn tokenize(d: &str) -> Vec<Token> {
        let bytes = d.as_bytes();
        let mut tokens = Vec::new();
        let mut index = 0;
        while index < bytes.len() {
            let byte = bytes[index];
            if byte.is_ascii_whitespace() || byte == b',' {
                index += 1;
                continue;
            }
            if byte.is_ascii_alphabetic() {
                tokens.push(Token::Command(byte as char));
                index += 1;
                continue;
            }
            let start = index;
            if byte == b'+' || byte == b'-' {
                index += 1;
            }
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            if index < bytes.len() && bytes[index] == b'.' {
                index += 1;
                while index < bytes.len() && bytes[index].is_ascii_digit() {
                    index += 1;
                }
            }
            if index < bytes.len() && (bytes[index] == b'e' || bytes[index] == b'E') {
                index += 1;
                if index < bytes.len() && (bytes[index] == b'+' || bytes[index] == b'-') {
                    index += 1;
                }
                while index < bytes.len() && bytes[index].is_ascii_digit() {
                    index += 1;
                }
            }
            let number = d[start..index].parse().expect("path number");
            tokens.push(Token::Number(number));
        }
        tokens
    }

    /// Normalizes an SVG `d` attribute into the subset these shapes emit: `M`, `L`, `H`, `V`,
    /// `A`, and `Z`, absolute or relative.
    fn parse_path(d: &str) -> Vec<Command> {
        let tokens = tokenize(d);
        let mut commands = Vec::new();
        let mut index = 0;
        let mut current = 'L';
        let mut x = 0.0;
        let mut y = 0.0;
        let mut start = (0.0, 0.0);
        let number = |index: &mut usize| -> f64 {
            match tokens.get(*index) {
                Some(Token::Number(value)) => {
                    *index += 1;
                    *value
                }
                _ => panic!("expected a path number"),
            }
        };
        while index < tokens.len() {
            if let Some(Token::Command(command)) = tokens.get(index) {
                current = *command;
                index += 1;
            }
            let relative = current.is_ascii_lowercase();
            let (ox, oy) = if relative { (x, y) } else { (0.0, 0.0) };
            match current.to_ascii_uppercase() {
                'M' => {
                    x = ox + number(&mut index);
                    y = oy + number(&mut index);
                    start = (x, y);
                    commands.push(Command::Move(x, y));
                    current = if relative { 'l' } else { 'L' };
                }
                'L' => {
                    x = ox + number(&mut index);
                    y = oy + number(&mut index);
                    commands.push(Command::Line(x, y));
                }
                'H' => {
                    x = ox + number(&mut index);
                    commands.push(Command::Line(x, y));
                }
                'V' => {
                    y = oy + number(&mut index);
                    commands.push(Command::Line(x, y));
                }
                'A' => {
                    let rx = number(&mut index);
                    let ry = number(&mut index);
                    let rotation = number(&mut index);
                    let large = number(&mut index) != 0.0;
                    let sweep = number(&mut index) != 0.0;
                    x = ox + number(&mut index);
                    y = oy + number(&mut index);
                    commands.push(Command::Arc {
                        rx,
                        ry,
                        rotation,
                        large,
                        sweep,
                        x,
                        y,
                    });
                }
                'Z' => {
                    x = start.0;
                    y = start.1;
                    commands.push(Command::Close);
                }
                other => panic!("unsupported fixture path command: {other}"),
            }
        }
        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_text_beside_images() {
        let content = LabelContent::Other(vec![
            LabelContent::Image,
            LabelContent::Text("  "),
            LabelContent::Other(vec![LabelContent::Text("label")]),
        ]);
        assert!(has_text_besides_images(&content));
        assert!(!has_text_besides_images(&LabelContent::Other(vec![
            LabelContent::Image
        ])));
        assert!(has_text_besides_images(&LabelContent::Text("x")));
        assert!(!has_text_besides_images(&LabelContent::Image));
    }

    #[test]
    fn parses_font_sizes_like_int_parse() {
        assert_eq!(parse_font_size("16px"), Some(16.0));
        assert_eq!(parse_font_size("16"), Some(16.0));
        assert_eq!(parse_font_size("  16.9px"), Some(16.0));
        assert_eq!(parse_font_size("-4px"), Some(-4.0));
        assert_eq!(parse_font_size("px"), None);
        assert_eq!(parse_font_size(""), None);
        assert_eq!(resolve_font_size(None, Some("20px"), 16.0), 20.0);
        assert_eq!(resolve_font_size(Some("12px"), Some("20px"), 16.0), 12.0);
        assert_eq!(resolve_font_size(None, Some("bad"), 16.0), 16.0);
        assert_eq!(resolve_font_size(None, None, 16.0), 16.0);
    }

    #[test]
    fn sizes_images_by_label_text() {
        assert_eq!(image_width(true, 16.0), ImageWidth::Fixed(80.0));
        assert_eq!(image_width(false, 16.0), ImageWidth::Container);
        let fixed = configured_image_layout(true, 16.0);
        assert_eq!(fixed.display, "flex");
        assert_eq!(fixed.flex_direction, "column");
        assert_eq!(fixed.width_percent, None);
        assert_eq!(fixed.min_width_px, Some(80));
        assert_eq!(fixed.max_width_px, Some(80));
        let container = configured_image_layout(false, 16.0);
        assert_eq!(container.width_percent, Some(100));
        assert_eq!(container.min_width_px, None);
    }
}
