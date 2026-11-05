//! The canvas engine's geometry (`@polterware/frontend-canvas`): the viewport's pan and zoom,
//! the orthogonal route a connection takes between two boxes, and hit testing.

/// A point in document or screen units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

impl Pt {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// A box in document units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Box2 {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Box2 {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains(&self, at: Pt) -> bool {
        at.x >= self.x
            && at.x <= self.x + self.width
            && at.y >= self.y
            && at.y <= self.y + self.height
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

/// `MIN_ZOOM` and `MAX_ZOOM`.
pub const MIN_ZOOM: f32 = 0.25;
pub const MAX_ZOOM: f32 = 2.5;

/// Where the document sits on screen: a document point `p` lands at `p * zoom + (x, y)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
        }
    }
}

pub fn clamp_zoom(zoom: f32) -> f32 {
    if zoom.is_finite() {
        zoom.clamp(MIN_ZOOM, MAX_ZOOM)
    } else {
        1.0
    }
}

impl Viewport {
    pub fn to_document(&self, screen: Pt) -> Pt {
        Pt::new(
            (screen.x - self.x) / self.zoom,
            (screen.y - self.y) / self.zoom,
        )
    }

    pub fn to_screen(&self, document: Pt) -> Pt {
        Pt::new(
            document.x * self.zoom + self.x,
            document.y * self.zoom + self.y,
        )
    }

    pub fn pan_by(&self, dx: f32, dy: f32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            ..*self
        }
    }

    /// Zooms keeping the document point under `focus` fixed on screen.
    pub fn zoom_at(&self, focus: Pt, zoom: f32) -> Self {
        let zoom = clamp_zoom(zoom);
        let at = self.to_document(focus);
        Self {
            zoom,
            x: focus.x - at.x * zoom,
            y: focus.y - at.y * zoom,
        }
    }

    /// Fits `bounds` (`min_x, min_y, max_x, max_y`) in a view, `padding` in, never past 100%.
    pub fn fit(bounds: (f32, f32, f32, f32), width: f32, height: f32, padding: f32) -> Self {
        let (min_x, min_y, max_x, max_y) = bounds;
        let w = (max_x - min_x).max(1.0);
        let h = (max_y - min_y).max(1.0);
        let zoom = clamp_zoom(
            ((width - padding * 2.0) / w)
                .min((height - padding * 2.0) / h)
                .min(1.0),
        );
        Self {
            zoom,
            x: width / 2.0 - (min_x + max_x) / 2.0 * zoom,
            y: height / 2.0 - (min_y + max_y) / 2.0 * zoom,
        }
    }
}

/// The bounds of boxes, or the 800×600 page an empty document has (`documentBounds`).
pub fn bounds_of(boxes: impl IntoIterator<Item = Box2>) -> (f32, f32, f32, f32) {
    let mut out: Option<(f32, f32, f32, f32)> = None;
    for item in boxes {
        out = Some(match out {
            None => (item.x, item.y, item.right(), item.bottom()),
            Some((a, b, c, d)) => (
                a.min(item.x),
                b.min(item.y),
                c.max(item.right()),
                d.max(item.bottom()),
            ),
        });
    }
    out.unwrap_or((0.0, 0.0, 800.0, 600.0))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

fn anchor(rect: Box2, side: Side) -> Pt {
    match side {
        Side::Left => Pt::new(rect.x, rect.y + rect.height / 2.0),
        Side::Right => Pt::new(rect.right(), rect.y + rect.height / 2.0),
        Side::Top => Pt::new(rect.x + rect.width / 2.0, rect.y),
        Side::Bottom => Pt::new(rect.x + rect.width / 2.0, rect.bottom()),
    }
}

/// The automatic orthogonal route from `a` to `b` (`resolveAutomaticConnection`): out of the
/// side facing the other box, an elbow halfway, and into the opposite side.
pub fn route(a: Box2, b: Box2) -> Vec<Pt> {
    let dx = (b.x + b.width / 2.0) - (a.x + a.width / 2.0);
    let dy = (b.y + b.height / 2.0) - (a.y + a.height / 2.0);
    let (from, to) = if dx.abs() >= dy.abs() {
        if dx >= 0.0 {
            (Side::Right, Side::Left)
        } else {
            (Side::Left, Side::Right)
        }
    } else if dy >= 0.0 {
        (Side::Bottom, Side::Top)
    } else {
        (Side::Top, Side::Bottom)
    };
    let source = anchor(a, from);
    let target = anchor(b, to);
    let mut points = vec![source];
    if matches!(from, Side::Left | Side::Right) {
        let middle = (source.x + target.x) / 2.0;
        points.push(Pt::new(middle, source.y));
        points.push(Pt::new(middle, target.y));
    } else {
        let middle = (source.y + target.y) / 2.0;
        points.push(Pt::new(source.x, middle));
        points.push(Pt::new(target.x, middle));
    }
    points.push(target);
    points
}

/// How far from a connection a press still picks it (`edgeHitPadding`).
pub const EDGE_HIT_PADDING: f32 = 14.0;

fn segment_distance_sq(p: Pt, a: Pt, b: Pt) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = dx * dx + dy * dy;
    let t = if len == 0.0 {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len).clamp(0.0, 1.0)
    };
    let (cx, cy) = (a.x + t * dx, a.y + t * dy);
    (p.x - cx).powi(2) + (p.y - cy).powi(2)
}

/// Whether `p` lies within `pad` of the polyline.
pub fn near_polyline(p: Pt, points: &[Pt], pad: f32) -> bool {
    points
        .windows(2)
        .any(|pair| segment_distance_sq(p, pair[0], pair[1]) <= pad * pad)
}

/// What a press lands on, nodes first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hit {
    Node(String),
    Edge(String),
    Empty,
}

/// Hits the top-most node under `at` (highest `z` first), then the last-drawn connection
/// near it (`hitTest` without ports).
pub fn hit_test<'a>(
    nodes: impl IntoIterator<Item = (&'a str, Box2, i32)>,
    edges: impl IntoIterator<Item = (&'a str, Vec<Pt>)>,
    at: Pt,
) -> Hit {
    let mut nodes: Vec<_> = nodes.into_iter().collect();
    // A stable sort, as JavaScript's: of two nodes at one `z`, the earlier one wins.
    nodes.sort_by(|a, b| b.2.cmp(&a.2));
    if let Some((id, ..)) = nodes.iter().find(|(_, rect, _)| rect.contains(at)) {
        return Hit::Node(id.to_string());
    }
    let edges: Vec<_> = edges.into_iter().collect();
    for (id, points) in edges.iter().rev() {
        if near_polyline(at, points, EDGE_HIT_PADDING) {
            return Hit::Edge(id.to_string());
        }
    }
    Hit::Empty
}

/// Places boxes left to right from (40, 40), `gap` apart (`layoutSync`, horizontal).
pub fn arrange_horizontally(widths: &[f32], gap: f32) -> Vec<Pt> {
    let mut cursor = 40.0;
    widths
        .iter()
        .map(|width| {
            let at = Pt::new(cursor, 40.0);
            cursor += width + gap;
            at
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_route_leaves_the_facing_side_and_bends_halfway() {
        let a = Box2::new(0.0, 0.0, 100.0, 50.0);
        let b = Box2::new(300.0, 100.0, 100.0, 50.0);
        assert_eq!(
            route(a, b),
            vec![
                Pt::new(100.0, 25.0),
                Pt::new(200.0, 25.0),
                Pt::new(200.0, 125.0),
                Pt::new(300.0, 125.0),
            ]
        );
    }

    #[test]
    fn zooming_keeps_the_focus_still() {
        let view = Viewport {
            x: 40.0,
            y: 40.0,
            zoom: 1.0,
        };
        let focus = Pt::new(200.0, 100.0);
        let before = view.to_document(focus);
        let zoomed = view.zoom_at(focus, 2.0);
        let after = zoomed.to_document(focus);
        assert!((before.x - after.x).abs() < 1e-4 && (before.y - after.y).abs() < 1e-4);
        assert_eq!(view.zoom_at(focus, 9.0).zoom, MAX_ZOOM);
    }

    #[test]
    fn the_top_node_wins_and_edges_come_after_nodes() {
        let nodes = [
            ("under", Box2::new(0.0, 0.0, 100.0, 100.0), 0),
            ("over", Box2::new(50.0, 50.0, 100.0, 100.0), 1),
        ];
        assert_eq!(
            hit_test(nodes, [], Pt::new(60.0, 60.0)),
            Hit::Node("over".into())
        );
        let edge = ("link", vec![Pt::new(200.0, 0.0), Pt::new(400.0, 0.0)]);
        assert_eq!(
            hit_test(nodes, [edge], Pt::new(300.0, 10.0)),
            Hit::Edge("link".into())
        );
    }

    #[test]
    fn fitting_never_zooms_past_one() {
        let view = Viewport::fit((0.0, 0.0, 100.0, 100.0), 1000.0, 800.0, 48.0);
        assert_eq!(view.zoom, 1.0);
        assert_eq!((view.x, view.y), (450.0, 350.0));
    }
}
