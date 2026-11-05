//! SVG path data (`d`) drawn with GPUI's paths: the drawing library and the component
//! silhouettes are written as path data in a w×h box at the origin, as `shapes.ts` writes
//! them, and are scaled and moved onto the canvas when painted.
//!
//! The commands are SVG's: `M L H V C Q T A Z` and their relative forms, with implicit
//! repeats. A stroke keeps its width on screen whatever the scale (`vector-effect:
//! non-scaling-stroke`), since GPUI tessellates after the transform. A fill follows SVG's
//! nonzero rule, so a closed detail inside an outline (a platform's slots) stays filled.

use gpui::{FillOptions, Path, PathBuilder, PathStyle, Pixels, Point, point, px};

/// One drawing command, in absolute coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    Move(f32, f32),
    Line(f32, f32),
    Quad {
        ctrl: (f32, f32),
        to: (f32, f32),
    },
    Cubic {
        a: (f32, f32),
        b: (f32, f32),
        to: (f32, f32),
    },
    Arc {
        radii: (f32, f32),
        rotation: f32,
        large: bool,
        sweep: bool,
        to: (f32, f32),
    },
    Close,
}

/// Reads path data into absolute segments. Unknown text ends the path where it stands.
pub fn parse(d: &str) -> Vec<Segment> {
    let mut tokens = Tokens::new(d);
    let mut out = Vec::new();
    let (mut x, mut y) = (0.0_f32, 0.0_f32);
    let (mut start_x, mut start_y) = (0.0_f32, 0.0_f32);
    // The last quadratic control, reflected by `T`.
    let mut last_quad: Option<(f32, f32)> = None;
    let mut command = None;
    loop {
        let next = match tokens.command() {
            Some(letter) => Some(letter),
            None if tokens.has_number() => command.map(|letter| match letter {
                'M' => 'L',
                'm' => 'l',
                other => other,
            }),
            None => None,
        };
        let Some(letter) = next else { break };
        command = Some(letter);
        let relative = letter.is_ascii_lowercase();
        let (ox, oy) = if relative { (x, y) } else { (0.0, 0.0) };
        let mut quad = None;
        match letter.to_ascii_uppercase() {
            'M' => {
                let Some((px_, py_)) = tokens.pair() else {
                    break;
                };
                (x, y) = (ox + px_, oy + py_);
                (start_x, start_y) = (x, y);
                out.push(Segment::Move(x, y));
            }
            'L' => {
                let Some((px_, py_)) = tokens.pair() else {
                    break;
                };
                (x, y) = (ox + px_, oy + py_);
                out.push(Segment::Line(x, y));
            }
            'H' => {
                let Some(value) = tokens.number() else { break };
                x = ox + value;
                out.push(Segment::Line(x, y));
            }
            'V' => {
                let Some(value) = tokens.number() else { break };
                y = if relative { y + value } else { value };
                out.push(Segment::Line(x, y));
            }
            'Q' => {
                let (Some(c), Some(to)) = (tokens.pair(), tokens.pair()) else {
                    break;
                };
                let ctrl = (ox + c.0, oy + c.1);
                (x, y) = (ox + to.0, oy + to.1);
                quad = Some(ctrl);
                out.push(Segment::Quad { ctrl, to: (x, y) });
            }
            'T' => {
                let Some(to) = tokens.pair() else { break };
                let ctrl = last_quad.map_or((x, y), |(cx, cy)| (2.0 * x - cx, 2.0 * y - cy));
                (x, y) = (ox + to.0, oy + to.1);
                quad = Some(ctrl);
                out.push(Segment::Quad { ctrl, to: (x, y) });
            }
            'C' => {
                let (Some(a), Some(b), Some(to)) = (tokens.pair(), tokens.pair(), tokens.pair())
                else {
                    break;
                };
                let a = (ox + a.0, oy + a.1);
                let b = (ox + b.0, oy + b.1);
                (x, y) = (ox + to.0, oy + to.1);
                out.push(Segment::Cubic { a, b, to: (x, y) });
            }
            'A' => {
                let (Some(rx), Some(ry), Some(rotation), Some(large), Some(sweep), Some(to)) = (
                    tokens.number(),
                    tokens.number(),
                    tokens.number(),
                    tokens.flag(),
                    tokens.flag(),
                    tokens.pair(),
                ) else {
                    break;
                };
                (x, y) = (ox + to.0, oy + to.1);
                out.push(Segment::Arc {
                    radii: (rx.abs(), ry.abs()),
                    rotation,
                    large,
                    sweep,
                    to: (x, y),
                });
            }
            'Z' => {
                (x, y) = (start_x, start_y);
                out.push(Segment::Close);
            }
            _ => break,
        }
        last_quad = quad;
    }
    out
}

/// The closed subpaths alone: an open detail such as a cylinder's lid would close against
/// its chord when filled and punch a hole (`outline()` in `node-renderer.tsx`).
pub fn closed_only(segments: &[Segment]) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut current: Vec<Segment> = Vec::new();
    for segment in segments {
        if matches!(segment, Segment::Move(..)) && !current.is_empty() {
            current.clear();
        }
        current.push(*segment);
        if matches!(segment, Segment::Close) {
            out.append(&mut current);
        }
    }
    out
}

/// Where and how large a path in document units lands on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// The screen point the path's origin lands on.
    pub origin: Point<Pixels>,
    /// Screen pixels per document unit.
    pub scale: f32,
}

impl Placement {
    pub fn new(origin: Point<Pixels>, scale: f32) -> Self {
        Self { origin, scale }
    }

    pub fn at(&self, x: f32, y: f32) -> Point<Pixels> {
        point(
            self.origin.x + px(x * self.scale),
            self.origin.y + px(y * self.scale),
        )
    }
}

/// How a path is painted: filled, or stroked `width` screen pixels wide, maybe dashed with
/// `dash` screen pixels on and off.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    Fill,
    Stroke { width: f32, dash: Option<Vec<f32>> },
}

/// Builds the segments as a GPUI path placed on screen, or `None` for an empty one.
pub fn build(segments: &[Segment], placement: Placement, paint: &Paint) -> Option<Path<Pixels>> {
    if segments.is_empty() {
        return None;
    }
    let mut builder = match paint {
        Paint::Fill => PathBuilder::fill().with_style(PathStyle::Fill(FillOptions::non_zero())),
        Paint::Stroke { width, dash } => {
            let builder = PathBuilder::stroke(px(*width));
            match dash {
                Some(dash) if !dash.is_empty() => {
                    builder.dash_array(&dash.iter().map(|value| px(*value)).collect::<Vec<_>>())
                }
                _ => builder,
            }
        }
    };
    let at = |(x, y): (f32, f32)| placement.at(x, y);
    for segment in segments {
        match *segment {
            Segment::Move(x, y) => builder.move_to(at((x, y))),
            Segment::Line(x, y) => builder.line_to(at((x, y))),
            Segment::Quad { ctrl, to } => builder.curve_to(at(to), at(ctrl)),
            Segment::Cubic { a, b, to } => builder.cubic_bezier_to(at(to), at(a), at(b)),
            Segment::Arc {
                radii,
                rotation,
                large,
                sweep,
                to,
            } => builder.arc_to(
                point(px(radii.0 * placement.scale), px(radii.1 * placement.scale)),
                px(rotation),
                large,
                sweep,
                at(to),
            ),
            Segment::Close => builder.close(),
        }
    }
    builder.build().ok()
}

/// The bounds of the segments' points and controls, as `(min_x, min_y, max_x, max_y)`.
pub fn extent(segments: &[Segment]) -> Option<(f32, f32, f32, f32)> {
    let mut points = segments.iter().flat_map(|segment| match *segment {
        Segment::Move(x, y) | Segment::Line(x, y) => vec![(x, y)],
        Segment::Quad { ctrl, to } => vec![ctrl, to],
        Segment::Cubic { a, b, to } => vec![a, b, to],
        Segment::Arc { to, .. } => vec![to],
        Segment::Close => Vec::new(),
    });
    let first = points.next()?;
    Some(points.fold(
        (first.0, first.1, first.0, first.1),
        |(a, b, c, d), (x, y)| (a.min(x), b.min(y), c.max(x), d.max(y)),
    ))
}

struct Tokens<'a> {
    text: &'a [u8],
    at: usize,
}

impl<'a> Tokens<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text: text.as_bytes(),
            at: 0,
        }
    }

    fn skip(&mut self) {
        while self.at < self.text.len()
            && matches!(self.text[self.at], b' ' | b',' | b'\n' | b'\t' | b'\r')
        {
            self.at += 1;
        }
    }

    fn command(&mut self) -> Option<char> {
        self.skip();
        let byte = *self.text.get(self.at)?;
        if byte.is_ascii_alphabetic() && byte != b'e' && byte != b'E' {
            self.at += 1;
            Some(byte as char)
        } else {
            None
        }
    }

    fn has_number(&mut self) -> bool {
        self.skip();
        self.text
            .get(self.at)
            .is_some_and(|byte| byte.is_ascii_digit() || matches!(byte, b'-' | b'+' | b'.'))
    }

    fn number(&mut self) -> Option<f32> {
        self.skip();
        let start = self.at;
        let mut end = start;
        if matches!(self.text.get(end), Some(b'-' | b'+')) {
            end += 1;
        }
        let mut dot = false;
        while let Some(byte) = self.text.get(end) {
            match byte {
                b'0'..=b'9' => end += 1,
                b'.' if !dot => {
                    dot = true;
                    end += 1;
                }
                b'e' | b'E' => {
                    end += 1;
                    if matches!(self.text.get(end), Some(b'-' | b'+')) {
                        end += 1;
                    }
                }
                _ => break,
            }
        }
        let value = std::str::from_utf8(&self.text[start..end])
            .ok()?
            .parse()
            .ok()?;
        self.at = end;
        Some(value)
    }

    /// An arc flag: one digit, which SVG lets touch the next number.
    fn flag(&mut self) -> Option<bool> {
        self.skip();
        let byte = *self.text.get(self.at)?;
        self.at += 1;
        match byte {
            b'0' => Some(false),
            b'1' => Some(true),
            _ => None,
        }
    }

    fn pair(&mut self) -> Option<(f32, f32)> {
        Some((self.number()?, self.number()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_read_as_absolute_segments() {
        let path = parse("M10 0H90A10 10 0 0 1 100 10V40L0 50ZM0 0q5 5 10 0t10 0");
        assert_eq!(path[0], Segment::Move(10.0, 0.0));
        assert_eq!(path[1], Segment::Line(90.0, 0.0));
        assert_eq!(
            path[2],
            Segment::Arc {
                radii: (10.0, 10.0),
                rotation: 0.0,
                large: false,
                sweep: true,
                to: (100.0, 10.0),
            }
        );
        assert_eq!(path[3], Segment::Line(100.0, 40.0));
        assert_eq!(path[5], Segment::Close);
        // The second quadratic reflects the first one's control.
        assert_eq!(
            path[7],
            Segment::Quad {
                ctrl: (5.0, 5.0),
                to: (10.0, 0.0)
            }
        );
        assert_eq!(
            path[8],
            Segment::Quad {
                ctrl: (15.0, -5.0),
                to: (20.0, 0.0)
            }
        );
    }

    #[test]
    fn a_fill_keeps_only_the_closed_subpaths() {
        // A cylinder: the closed body, then its open lid.
        let path = parse("M0 16A60 16 0 0 1 120 16V124A60 16 0 0 1 0 124ZM0 16A60 16 0 0 0 120 16");
        let closed = closed_only(&path);
        assert_eq!(closed.len(), 5);
        assert_eq!(closed.last(), Some(&Segment::Close));
    }

    #[test]
    fn implicit_repeats_and_spaced_edges_parse() {
        let path = parse("M 0 0 L 10 0 L 10 10 20 10");
        assert_eq!(path.len(), 4);
        assert_eq!(path[3], Segment::Line(20.0, 10.0));
        assert_eq!(extent(&path), Some((0.0, 0.0, 20.0, 10.0)));
    }
}
